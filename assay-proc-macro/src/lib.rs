/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
  parse::{Parse, ParseStream},
  parse_macro_input,
  punctuated::Punctuated,
  Expr, ExprArray, ExprLit, ExprTuple, ExprUnary, FnArg, Ident, ItemFn, Lit, Pat, Result, Token,
  UnOp,
};

/// A named test case with arguments
struct NamedCase {
  name: Ident,
  args: ExprTuple,
}

/// A matrix parameter with multiple values for combinatorial testing
struct MatrixParam {
  name: Ident,
  values: Vec<Expr>,
}

/// Convert an expression to a valid identifier component for test naming.
/// Returns None if the expression is too complex (fallback to index).
fn expr_to_ident_component(expr: &Expr) -> Option<String> {
  match expr {
    // Integer literals: 42 → "42"
    Expr::Lit(ExprLit {
      lit: Lit::Int(lit), ..
    }) => Some(lit.base10_digits().to_string()),

    // Negative integers: -5 → "neg5"
    Expr::Unary(ExprUnary {
      op: UnOp::Neg(_),
      expr,
      ..
    }) => {
      if let Expr::Lit(ExprLit {
        lit: Lit::Int(lit), ..
      }) = expr.as_ref()
      {
        Some(format!("neg{}", lit.base10_digits()))
      } else {
        None
      }
    }

    // String literals: "foo" → "foo", "foo-bar" → "foo_bar"
    Expr::Lit(ExprLit {
      lit: Lit::Str(lit), ..
    }) => {
      let s = lit.value();
      let sanitized: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();

      if sanitized.is_empty() {
        return None;
      }
      // Can't start with digit
      if sanitized.chars().next().unwrap().is_ascii_digit() {
        return Some(format!("_{}", sanitized));
      }
      Some(sanitized)
    }

    // Bool literals: true → "true", false → "false"
    Expr::Lit(ExprLit {
      lit: Lit::Bool(lit),
      ..
    }) => Some(lit.value.to_string()),

    // Everything else: too complex, use index
    _ => None,
  }
}

/// Compute Cartesian product of value lists
fn cartesian_product<T: Clone>(lists: &[Vec<T>]) -> Vec<Vec<T>> {
  if lists.is_empty() {
    return vec![vec![]];
  }

  let mut result = vec![vec![]];

  for list in lists {
    let mut new_result = Vec::new();
    for existing in &result {
      for item in list {
        let mut new_combo = existing.clone();
        new_combo.push(item.clone());
        new_result.push(new_combo);
      }
    }
    result = new_result;
  }

  result
}

/// Parse a duration string like "30s", "500ms", "2m" into milliseconds.
fn parse_duration(s: &str) -> std::result::Result<u64, String> {
  let s = s.trim();

  if s.is_empty() {
    return Err("duration cannot be empty".to_string());
  }

  // Try to split into number and unit
  let (num_str, unit) = if let Some(idx) = s.find(|c: char| !c.is_ascii_digit()) {
    (&s[..idx], s[idx..].trim())
  } else {
    (s, "") // bare number, default to seconds
  };

  if num_str.is_empty() {
    return Err(format!("invalid duration '{}': missing number", s));
  }

  let num: u64 = num_str
    .parse()
    .map_err(|_| format!("invalid number in duration: '{}'", num_str))?;

  let millis = match unit.to_lowercase().as_str() {
    "s" | "sec" | "secs" | "second" | "seconds" => num.checked_mul(1000),
    "ms" | "milli" | "millis" | "millisecond" | "milliseconds" => Some(num),
    "m" | "min" | "mins" | "minute" | "minutes" => num.checked_mul(60 * 1000),
    "" => num.checked_mul(1000), // bare number = seconds
    other => {
      return Err(format!(
        "unknown duration unit: '{}'\nvalid units: s, ms, m",
        other
      ))
    }
  };

  let millis = millis.ok_or_else(|| "timeout duration overflow".to_string())?;

  if millis == 0 {
    return Err("timeout cannot be zero".to_string());
  }

  Ok(millis)
}

struct AssayAttribute {
  /// (source_path, optional_dest_path)
  /// If dest is None, file is copied to temp root with its filename only
  include: Option<Vec<(String, Option<String>)>>,
  ignore: bool,
  should_panic: bool,
  env: Option<Vec<(String, String)>>,
  setup: Option<Expr>,
  teardown: Option<Expr>,
  /// Timeout in milliseconds
  timeout: Option<u64>,
  /// Number of retry attempts (1 = run once, 2 = one retry, etc.)
  retries: Option<u32>,
  /// Named test cases for parameterized testing
  cases: Option<Vec<NamedCase>>,
  /// Matrix parameters for combinatorial testing
  matrix: Option<Vec<MatrixParam>>,
}

impl Parse for AssayAttribute {
  fn parse(input: ParseStream) -> Result<Self> {
    let mut include = None;
    let mut ignore = false;
    let mut should_panic = false;
    let mut env = None;
    let mut setup = None;
    let mut teardown = None;
    let mut timeout = None;
    let mut retries = None;
    let mut cases = None;
    let mut matrix = None;

    while input.peek(Ident) || {
      if input.peek(Token![,]) {
        let _: Token![,] = input.parse()?;
      }
      input.peek(Ident)
    } {
      let ident: Ident = input.parse()?;
      match ident.to_string().as_str() {
        "include" => {
          if include.is_some() {
            return Err(syn::Error::new_spanned(
              &ident,
              "duplicate `include` attribute\nhelp: combine all files into a single array: `include = [\"file1\", \"file2\"]`"
            ));
          }

          input.parse::<Token![=]>().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected `=` after `include`\nhelp: use `include = [\"file.txt\"]`",
            )
          })?;
          let array: ExprArray = input.parse().map_err(|e| {
            syn::Error::new(e.span(), "expected array after `include =`\nhelp: use `include = [\"file.txt\", \"other.txt\"]`")
          })?;

          if array.elems.is_empty() {
            return Err(syn::Error::new_spanned(
              &array,
              "include array cannot be empty\nhelp: provide at least one file path, e.g., `include = [\"Cargo.toml\"]`"
            ));
          }

          let mut files = Vec::new();
          for elem in array.elems {
            match elem {
              // String literal: copy to temp root with filename only
              Expr::Lit(ExprLit {
                lit: Lit::Str(lit_str),
                ..
              }) => {
                files.push((lit_str.value(), None));
              }
              // Tuple: (source, dest) for custom destination
              Expr::Tuple(tuple) => {
                if tuple.elems.len() != 2 {
                  return Err(syn::Error::new_spanned(
                    &tuple,
                    format!(
                      "include tuple must have exactly 2 elements (source, dest), found {}\nhelp: use `(\"source.txt\", \"dest.txt\")` format",
                      tuple.elems.len()
                    )
                  ));
                }

                let source = match &tuple.elems[0] {
                  Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                  }) => s.value(),
                  other => {
                    return Err(syn::Error::new_spanned(
                      other,
                      "include tuple source must be a string literal\nhelp: use `(\"source.txt\", \"dest.txt\")` format",
                    ));
                  }
                };

                let dest = match &tuple.elems[1] {
                  Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                  }) => s.value(),
                  other => {
                    return Err(syn::Error::new_spanned(
                      other,
                      "include tuple destination must be a string literal\nhelp: use `(\"source.txt\", \"dest.txt\")` format",
                    ));
                  }
                };

                files.push((source, Some(dest)));
              }
              other => {
                return Err(syn::Error::new_spanned(
                  &other,
                  "include array elements must be string literals or tuples\nhelp: use `include = [\"file.txt\"]` or `include = [(\"source.txt\", \"dest.txt\")]`"
                ));
              }
            }
          }

          include = Some(files);
        }
        "should_panic" => {
          if should_panic {
            return Err(syn::Error::new_spanned(
              &ident,
              "duplicate `should_panic` attribute",
            ));
          }
          should_panic = true;
        }
        "ignore" => {
          if ignore {
            return Err(syn::Error::new_spanned(
              &ident,
              "duplicate `ignore` attribute",
            ));
          }
          ignore = true;
        }
        "env" => {
          if env.is_some() {
            return Err(syn::Error::new_spanned(
              &ident,
              "duplicate `env` attribute\nhelp: combine all variables into a single array: `env = [(\"K1\", \"v1\"), (\"K2\", \"v2\")]`"
            ));
          }

          input.parse::<Token![=]>().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected `=` after `env`\nhelp: use `env = [(\"KEY\", \"value\")]`",
            )
          })?;
          let array: ExprArray = input.parse().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected array after `env =`\nhelp: use `env = [(\"KEY\", \"value\")]`",
            )
          })?;

          if array.elems.is_empty() {
            return Err(syn::Error::new_spanned(
              &array,
              "env array cannot be empty\nhelp: provide at least one environment variable, e.g., `env = [(\"KEY\", \"value\")]`"
            ));
          }

          let mut env_vars = Vec::new();
          for elem in array.elems {
            match &elem {
              Expr::Tuple(tuple) => {
                if tuple.elems.len() != 2 {
                  return Err(syn::Error::new_spanned(
                    &elem,
                    format!(
                      "env tuple must have exactly 2 elements (key, value), found {}\nhelp: use `(\"KEY\", \"value\")` format",
                      tuple.elems.len()
                    )
                  ));
                }

                let key = match &tuple.elems[0] {
                  Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                  }) => s.value(),
                  other => {
                    return Err(syn::Error::new_spanned(
                      other,
                      "env key must be a string literal\nhelp: use `(\"KEY\", \"value\")` format",
                    ));
                  }
                };

                let value = match &tuple.elems[1] {
                  Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                  }) => s.value(),
                  other => {
                    return Err(syn::Error::new_spanned(
                      other,
                      "env value must be a string literal\nhelp: use `(\"KEY\", \"value\")` format",
                    ));
                  }
                };

                env_vars.push((key, value));
              }
              other => {
                return Err(syn::Error::new_spanned(
                  other,
                  "env array elements must be tuples of (key, value)\nhelp: use `env = [(\"KEY1\", \"value1\"), (\"KEY2\", \"value2\")]`"
                ));
              }
            }
          }

          env = Some(env_vars);
        }
        val @ "setup" | val @ "teardown" => {
          if val == "setup" {
            if setup.is_some() {
              return Err(syn::Error::new_spanned(
                &ident,
                "duplicate `setup` attribute",
              ));
            }
          } else if teardown.is_some() {
            return Err(syn::Error::new_spanned(
              &ident,
              "duplicate `teardown` attribute",
            ));
          }

          input.parse::<Token![=]>().map_err(|e| {
            syn::Error::new(
              e.span(),
              format!(
                "expected `=` after `{}`\nhelp: use `{} = my_function`",
                val, val
              ),
            )
          })?;
          let x = input.parse()?;
          if val == "setup" {
            setup = Some(x);
          } else {
            teardown = Some(x);
          }
        }
        "timeout" => {
          if timeout.is_some() {
            return Err(syn::Error::new_spanned(
              &ident,
              "duplicate `timeout` attribute",
            ));
          }

          input.parse::<Token![=]>().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected `=` after `timeout`\nhelp: use `timeout = \"30s\"`",
            )
          })?;

          let lit: syn::LitStr = input.parse().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected string after `timeout =`\nhelp: use `timeout = \"30s\"` or `timeout = \"500ms\"`",
            )
          })?;

          let duration_str = lit.value();
          let millis = parse_duration(&duration_str).map_err(|msg| {
            syn::Error::new_spanned(
              &lit,
              format!(
                "{}\nhelp: use `timeout = \"30s\"` or `timeout = \"500ms\"`",
                msg
              ),
            )
          })?;

          timeout = Some(millis);
        }
        "retries" => {
          if retries.is_some() {
            return Err(syn::Error::new_spanned(
              &ident,
              "duplicate `retries` attribute",
            ));
          }

          input.parse::<Token![=]>().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected `=` after `retries`\nhelp: use `retries = 3`",
            )
          })?;

          let lit: syn::LitInt = input.parse().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected integer after `retries =`\nhelp: use `retries = 3`",
            )
          })?;

          let count: u32 = lit.base10_parse().map_err(|_| {
            syn::Error::new_spanned(
              &lit,
              "retries must be a positive integer\nhelp: use `retries = 3`",
            )
          })?;

          if count == 0 {
            return Err(syn::Error::new_spanned(
              &lit,
              "retries cannot be zero\nhelp: use `retries = 1` to run once, or omit for default behavior",
            ));
          }

          retries = Some(count);
        }
        "cases" => {
          if cases.is_some() {
            return Err(syn::Error::new_spanned(
              &ident,
              "duplicate `cases` attribute",
            ));
          }
          if matrix.is_some() {
            return Err(syn::Error::new_spanned(
              &ident,
              "`cases` and `matrix` are mutually exclusive\nhelp: use one or the other, not both",
            ));
          }

          input.parse::<Token![=]>().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected `=` after `cases`\nhelp: use `cases = [name: (arg1, arg2)]`",
            )
          })?;

          let content;
          syn::bracketed!(content in input);

          let mut parsed_cases = Vec::new();

          while !content.is_empty() {
            // Parse: name: (args)
            let case_name: Ident = content.parse().map_err(|e| {
              syn::Error::new(
                e.span(),
                "expected case name\nhelp: use `cases = [my_case: (arg1, arg2)]`",
              )
            })?;

            content.parse::<Token![:]>().map_err(|e| {
              syn::Error::new(
                e.span(),
                "expected `:` after case name\nhelp: use `cases = [my_case: (arg1, arg2)]`",
              )
            })?;

            let args: ExprTuple = content.parse().map_err(|e| {
              syn::Error::new(
                e.span(),
                "expected tuple of arguments\nhelp: use `cases = [my_case: (arg1, arg2)]`",
              )
            })?;

            // Check for duplicate names
            if parsed_cases.iter().any(|c: &NamedCase| c.name == case_name) {
              return Err(syn::Error::new_spanned(
                &case_name,
                format!("duplicate case name `{}`", case_name),
              ));
            }

            parsed_cases.push(NamedCase {
              name: case_name,
              args,
            });

            // Optional trailing comma
            if content.peek(Token![,]) {
              content.parse::<Token![,]>()?;
            }
          }

          if parsed_cases.is_empty() {
            return Err(syn::Error::new_spanned(
              &ident,
              "cases cannot be empty\nhelp: provide at least one case, e.g., `cases = [my_case: (1, 2)]`",
            ));
          }

          cases = Some(parsed_cases);
        }
        "matrix" => {
          if matrix.is_some() {
            return Err(syn::Error::new_spanned(
              &ident,
              "duplicate `matrix` attribute",
            ));
          }
          if cases.is_some() {
            return Err(syn::Error::new_spanned(
              &ident,
              "`cases` and `matrix` are mutually exclusive\nhelp: use one or the other, not both",
            ));
          }

          input.parse::<Token![=]>().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected `=` after `matrix`\nhelp: use `matrix = [param: [val1, val2]]`",
            )
          })?;

          let content;
          syn::bracketed!(content in input);

          let mut params = Vec::new();

          while !content.is_empty() {
            // Parse: param_name: [val1, val2, ...]
            let param_name: Ident = content.parse().map_err(|e| {
              syn::Error::new(
                e.span(),
                "expected parameter name\nhelp: use `matrix = [param: [val1, val2]]`",
              )
            })?;

            content.parse::<Token![:]>().map_err(|e| {
              syn::Error::new(
                e.span(),
                "expected `:` after parameter name\nhelp: use `matrix = [param: [val1, val2]]`",
              )
            })?;

            let values_content;
            syn::bracketed!(values_content in content);

            let values: Punctuated<Expr, Token![,]> =
              values_content.parse_terminated(Expr::parse)?;
            let values: Vec<Expr> = values.into_iter().collect();

            if values.is_empty() {
              return Err(syn::Error::new_spanned(
                &param_name,
                format!(
                  "matrix parameter `{}` cannot have empty values\nhelp: provide at least one value",
                  param_name
                ),
              ));
            }

            // Check for duplicate parameter names
            if params.iter().any(|p: &MatrixParam| p.name == param_name) {
              return Err(syn::Error::new_spanned(
                &param_name,
                format!("duplicate matrix parameter `{}`", param_name),
              ));
            }

            params.push(MatrixParam {
              name: param_name,
              values,
            });

            // Optional trailing comma
            if content.peek(Token![,]) {
              content.parse::<Token![,]>()?;
            }
          }

          if params.is_empty() {
            return Err(syn::Error::new_spanned(
              &ident,
              "matrix cannot be empty\nhelp: provide at least one parameter, e.g., `matrix = [x: [1, 2]]`",
            ));
          }

          matrix = Some(params);
        }
        unknown => {
          let suggestion = match unknown {
            "includes" => Some("include"),
            "envs" | "environment" => Some("env"),
            "panic" | "panics" => Some("should_panic"),
            "ignored" => Some("ignore"),
            "set_up" | "before" | "before_each" => Some("setup"),
            "tear_down" | "after" | "after_each" | "cleanup" => Some("teardown"),
            "time" | "time_out" | "timelimit" | "time_limit" => Some("timeout"),
            "retry" | "attempts" | "tries" | "repeat" | "flaky" => Some("retries"),
            "case" | "params" | "parameters" | "test_cases" => Some("cases"),
            "values" | "combinations" | "cartesian" | "parametrize" => Some("matrix"),
            _ => None,
          };

          let valid_attrs =
            "include, ignore, should_panic, env, setup, teardown, timeout, retries, cases, matrix";

          let message = match suggestion {
            Some(suggested) => format!(
              "unknown attribute `{}`\nhelp: did you mean `{}`?\nvalid attributes are: {}",
              unknown, suggested, valid_attrs
            ),
            None => format!(
              "unknown attribute `{}`\nvalid attributes are: {}",
              unknown, valid_attrs
            ),
          };

          return Err(syn::Error::new_spanned(&ident, message));
        }
      }
    }

    Ok(AssayAttribute {
      include,
      ignore,
      should_panic,
      env,
      setup,
      teardown,
      timeout,
      retries,
      cases,
      matrix,
    })
  }
}

#[proc_macro_attribute]
pub fn assay(attr: TokenStream, item: TokenStream) -> TokenStream {
  let attr = parse_macro_input!(attr as AssayAttribute);

  let include = if let Some(include) = attr.include {
    let mut out = quote! {
      let fs = assay::PrivateFS::new()?;
    };
    for (source, dest) in include {
      out = match dest {
        Some(d) => quote! {
          #out
          fs.include_as(#source, #d)?;
        },
        None => quote! {
          #out
          fs.include(#source)?;
        },
      };
    }
    out
  } else {
    quote! {
      let fs = assay::PrivateFS::new()?;
    }
  };

  let ignore = if attr.ignore {
    quote! { #[ignore] }
  } else {
    quote! {}
  };

  let should_panic = if attr.should_panic {
    quote! { #[should_panic] }
  } else {
    quote! {}
  };

  let env = if let Some(env) = attr.env {
    let mut out = quote! {};
    for (k, v) in env {
      out = quote! {
        #out
        std::env::set_var(#k,#v);
      };
    }
    out
  } else {
    quote! {}
  };

  let setup = match attr.setup {
    Some(expr) => quote! { #expr; },
    None => quote! {},
  };
  let teardown = match attr.teardown {
    Some(expr) => quote! { #expr; },
    None => quote! {},
  };

  // Parse the function out into individual parts
  let func = parse_macro_input!(item as ItemFn);
  let vis = func.vis;
  let mut sig = func.sig;
  let name = sig.ident.clone();
  let asyncness = sig.asyncness.take();
  let block = func.block;
  let body = if asyncness.is_some() {
    #[cfg(not(feature = "async"))]
    compile_error!("You cannot use the async functionality in `assay` without specifiying a runtime. This error is occurring because you turned off the default features. Possible feature values are:\n- async-tokio-runtime\n- async-std-runtime");
    quote! {
      async fn inner_async() -> assay::Result<()> {
        #block
        Ok(())
      }
      assay::async_runtime::Runtime::block_on(inner_async())??;
    }
  } else {
    quote! { #block }
  };

  let fn_sig = if attr.should_panic {
    quote! { #vis #sig }
  } else {
    quote! { #vis #sig -> assay::Result<()> }
  };

  let ret = if attr.should_panic {
    quote! {}
  } else {
    quote! { Ok(()) }
  };

  let child = if attr.should_panic {
    quote! { child().unwrap() }
  } else {
    quote! { child() }
  };

  // For ignored tests, subprocess needs --ignored flag
  let subprocess_extra_args = if attr.ignore {
    quote! { .arg("--ignored") }
  } else {
    quote! {}
  };

  // Generate subprocess handling code - with or without timeout
  let subprocess_handling = if let Some(millis) = attr.timeout {
    // Format timeout for display (e.g., "30s" or "500ms")
    let timeout_display = if millis >= 1000 && millis % 1000 == 0 {
      format!("{}s", millis / 1000)
    } else {
      format!("{}ms", millis)
    };

    quote! {
      let binary = std::env::args().next().expect("no binary path in args");
      use assay::wait_timeout::ChildExt;
      use std::io::Read;

      let mut child = std::process::Command::new(&binary)
        .arg(&name)
        .arg("--exact")
        #subprocess_extra_args
        .env("ASSAY_SPLIT", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn subprocess");

      let timeout_duration = std::time::Duration::from_millis(#millis);
      let stdout = match child.wait_timeout(timeout_duration).expect("failed to wait on subprocess") {
        Some(_status) => {
          // Process completed within timeout
          let mut stdout = String::new();
          if let Some(ref mut out) = child.stdout {
            out.read_to_string(&mut stdout).ok();
          }
          stdout
        }
        None => {
          // Timeout! Kill the child process
          child.kill().expect("failed to kill timed-out subprocess");
          child.wait().expect("failed to wait after kill");
          panic!("test timed out after {}", #timeout_display);
        }
      };
    }
  } else {
    // No timeout - use original blocking .output()
    quote! {
      let binary = std::env::args().next().expect("no binary path in args");
      let out = std::process::Command::new(&binary)
        .arg(&name)
        .arg("--exact")
        #subprocess_extra_args
        .env("ASSAY_SPLIT", "1")
        .output()
        .expect("executed a subprocess");
      let stdout = String::from_utf8(out.stdout).unwrap();
    }
  };

  // Get retry count (1 = run once, no retries)
  let retry_count = attr.retries.unwrap_or(1);

  // Return type for generated test functions
  let ret_type = if attr.should_panic {
    quote! {}
  } else {
    quote! { -> assay::Result<()> }
  };

  // Extract parameter names from the function signature
  let param_names: Vec<Ident> = sig
    .inputs
    .iter()
    .filter_map(|arg| {
      if let FnArg::Typed(pat_type) = arg {
        if let Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
          return Some(pat_ident.ident.clone());
        }
      }
      None
    })
    .collect();

  // Helper to generate a single test function
  let generate_test = |test_fn_name: Ident, param_bindings: TokenStream2| -> TokenStream2 {
    // Generate subprocess handling with the correct test name
    let subprocess_handling_for_test = if let Some(millis) = attr.timeout {
      let timeout_display = if millis >= 1000 && millis % 1000 == 0 {
        format!("{}s", millis / 1000)
      } else {
        format!("{}ms", millis)
      };

      quote! {
        let binary = std::env::args().next().expect("no binary path in args");
        use assay::wait_timeout::ChildExt;
        use std::io::Read;

        let mut child = std::process::Command::new(&binary)
          .arg(&name)
          .arg("--exact")
          #subprocess_extra_args
          .env("ASSAY_SPLIT", "1")
          .stdout(std::process::Stdio::piped())
          .stderr(std::process::Stdio::piped())
          .spawn()
          .expect("failed to spawn subprocess");

        let timeout_duration = std::time::Duration::from_millis(#millis);
        let stdout = match child.wait_timeout(timeout_duration).expect("failed to wait on subprocess") {
          Some(_status) => {
            let mut stdout = String::new();
            if let Some(ref mut out) = child.stdout {
              out.read_to_string(&mut stdout).ok();
            }
            stdout
          }
          None => {
            child.kill().expect("failed to kill timed-out subprocess");
            child.wait().expect("failed to wait after kill");
            panic!("test timed out after {}", #timeout_display);
          }
        };
      }
    } else {
      quote! {
        let binary = std::env::args().next().expect("no binary path in args");
        let out = std::process::Command::new(&binary)
          .arg(&name)
          .arg("--exact")
          #subprocess_extra_args
          .env("ASSAY_SPLIT", "1")
          .output()
          .expect("executed a subprocess");
        let stdout = String::from_utf8(out.stdout).unwrap();
      }
    };

    let test_fn_name_for_body = test_fn_name.clone();

    quote! {
      #[test]
      #should_panic
      #ignore
      fn #test_fn_name() #ret_type {
        #[allow(unreachable_code)]
        fn child() -> assay::Result<()> {
          use assay::{assert_eq, assert_eq_sorted, assert_ne, net::TestAddress};
          #param_bindings
          #include
          #setup
          #env
          #body
          #teardown
          Ok(())
        }

        if std::env::var("NEXTEST_EXECUTION_MODE")
          .ok()
          .as_ref()
          .map(|s| s.as_str() == "process-per-test")
          .unwrap_or(false)
        {
          #child
        } else {
          let name = {
            let mut module = module_path!()
              .split("::")
              .into_iter()
              .skip(1)
              .collect::<Vec<_>>();
            module.push(stringify!(#test_fn_name_for_body));
            module.join("::")
          };
          if std::env::var("ASSAY_SPLIT")
              .as_ref()
              .map(|s| s.as_str() != "1")
              .unwrap_or(true)
          {
            let mut last_failure: Option<String> = None;
            for _attempt in 1..=#retry_count {
              #subprocess_handling_for_test
              if stdout.contains(&format!("{name} - should panic ... ok")) || stdout.contains(&format!("{name} ... FAILED")) {
                let stdout_line = format!("---- {name} stdout ----");
                let split = stdout
                  .lines()
                  .skip_while(|line| line != &stdout_line)
                  .skip(1)
                  .take_while(|s| !s.starts_with("----") && !s.starts_with("failures:"))
                  .collect::<Vec<&str>>()
                  .join("\n");
                last_failure = Some(split);
                continue;
              } else {
                last_failure = None;
                break;
              }
            }
            if let Some(failure) = last_failure {
              assay::panic_replace();
              panic!("ASSAY_PANIC_INTERNAL_MESSAGE\n{}", failure);
            }
            #ret
          } else {
            #child
          }
        }
      }
    }
  };

  // Handle cases, matrix, or regular test
  if let Some(cases) = attr.cases {
    // Generate a test for each named case
    let tests: Vec<_> = cases
      .into_iter()
      .map(|case| {
        let case_name = case.name;
        let test_fn_name = format_ident!("{}_{}", name, case_name);
        let args = case.args;

        // Destructure the tuple into parameter bindings
        let param_bindings = if param_names.is_empty() {
          quote! {}
        } else {
          quote! { let (#(#param_names),*) = #args; }
        };

        generate_test(test_fn_name, param_bindings)
      })
      .collect();

    TokenStream::from(quote! { #(#tests)* })
  } else if let Some(matrix_params) = attr.matrix {
    // Validate matrix params match function params
    let matrix_param_names: Vec<_> = matrix_params.iter().map(|p| &p.name).collect();

    if param_names.len() != matrix_param_names.len() {
      return syn::Error::new_spanned(
        &sig,
        format!(
          "matrix has {} parameters but function has {}\nhelp: matrix parameters must match function parameters",
          matrix_param_names.len(),
          param_names.len()
        ),
      )
      .to_compile_error()
      .into();
    }

    // Check names match (in order)
    for (fn_param, matrix_param) in param_names.iter().zip(matrix_param_names.iter()) {
      if fn_param != *matrix_param {
        return syn::Error::new_spanned(
          *matrix_param,
          format!(
            "matrix parameter `{}` doesn't match function parameter `{}`\nhelp: matrix parameters must match function parameters in order",
            matrix_param, fn_param
          ),
        )
        .to_compile_error()
        .into();
      }
    }

    // Get all value lists
    let value_lists: Vec<Vec<&Expr>> = matrix_params
      .iter()
      .map(|p| p.values.iter().collect())
      .collect();

    // Compute Cartesian product
    let combinations = cartesian_product(&value_lists);

    let tests: Vec<_> = combinations
      .into_iter()
      .map(|combo| {
        // Build test name from values
        let name_parts: Vec<String> = combo
          .iter()
          .enumerate()
          .map(|(i, expr)| expr_to_ident_component(expr).unwrap_or_else(|| i.to_string()))
          .collect();

        let test_suffix = name_parts.join("_");
        let test_fn_name = format_ident!("{}_{}", name, test_suffix);

        // Build parameter bindings
        let param_bindings = quote! {
          #(let #param_names = #combo;)*
        };

        generate_test(test_fn_name, param_bindings)
      })
      .collect();

    TokenStream::from(quote! { #(#tests)* })
  } else {
    // No parameterization - generate single test as before
    let expanded = quote! {
        #[test]
        #should_panic
        #ignore
        #fn_sig {
          #[allow(unreachable_code)]
          fn child() -> assay::Result<()> {
            use assay::{assert_eq, assert_eq_sorted, assert_ne, net::TestAddress};
            #include
            #setup
            #env
            #body
            #teardown
            Ok(())
          }

        if std::env::var("NEXTEST_EXECUTION_MODE")
          .ok()
          .as_ref()
          .map(|s| s.as_str() == "process-per-test")
          .unwrap_or(false)
        {
          // Note: timeout and retries attributes are ignored in nextest process-per-test mode
          // Configure via .config/nextest.toml: slow-timeout, leak-timeout, retries
          #child
        } else {
          let name = {
            let mut module = module_path!()
              .split("::")
              .into_iter()
              .skip(1)
              .collect::<Vec<_>>();
            module.push(stringify!(#name));
            module.join("::")
          };
          if std::env::var("ASSAY_SPLIT")
              .as_ref()
              .map(|s| s.as_str() != "1")
              .unwrap_or(true)
          {
            let mut last_failure: Option<String> = None;
            for _attempt in 1..=#retry_count {
              #subprocess_handling
              if stdout.contains(&format!("{name} - should panic ... ok")) || stdout.contains(&format!("{name} ... FAILED")) {
                let stdout_line = format!("---- {name} stdout ----");
                let split = stdout
                  .lines()
                  .skip_while(|line| line != &stdout_line)
                  .skip(1)
                  .take_while(|s| !s.starts_with("----") && !s.starts_with("failures:"))
                  .collect::<Vec<&str>>()
                  .join("\n");
                last_failure = Some(split);
                continue; // Retry
              } else {
                last_failure = None;
                break; // Success
              }
            }
            if let Some(failure) = last_failure {
              assay::panic_replace();
              panic!("ASSAY_PANIC_INTERNAL_MESSAGE\n{}", failure);
            }
            #ret
          } else{
            #child
          }
        }
      }
    };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
  }
}
