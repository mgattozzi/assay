/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use proc_macro::TokenStream;
use quote::quote;
use syn::{
  parse::{Parse, ParseStream},
  parse_macro_input, Expr, ExprArray, ExprLit, Ident, ItemFn, Lit, Result, Token,
};

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
            _ => None,
          };

          let valid_attrs = "include, ignore, should_panic, env, setup, teardown, timeout, retries";

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
