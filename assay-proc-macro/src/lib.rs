/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
  parse::{Parse, ParseStream},
  parse_macro_input,
  punctuated::Punctuated,
  spanned::Spanned,
  Expr, ExprArray, ExprLit, ExprTuple, ExprUnary, FnArg, Ident, ItemFn, Lit, Pat, Result, Token,
  UnOp,
};

// ============================================================
// Constants
// ============================================================

/// Environment variable set to "1" when running in subprocess mode
const ENV_ASSAY_SPLIT: &str = "ASSAY_SPLIT";

/// Milliseconds per second for duration formatting
const MILLIS_PER_SECOND: u64 = 1000;

/// Milliseconds per minute for duration parsing
const MILLIS_PER_MINUTE: u64 = 60_000;

/// Valid attribute names for the #[assay] macro
const VALID_ATTRIBUTES: &[&str] = &[
  "include", "env", "setup", "teardown", "timeout", "retries", "cases", "matrix",
];

// ============================================================
// Helper Structs for Parsing
// ============================================================

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

// ============================================================
// Parsing Utilities
// ============================================================

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

/// Error type for duration parsing, providing structured error information.
enum DurationError {
  Empty,
  MissingNumber,
  InvalidNumber(String),
  UnknownUnit(String),
  Overflow,
  Zero,
}

impl DurationError {
  fn message(&self) -> String {
    match self {
      Self::Empty => "duration cannot be empty".into(),
      Self::MissingNumber => "duration must start with a number".into(),
      Self::InvalidNumber(n) => format!("invalid number: '{}'", n),
      Self::UnknownUnit(u) => format!("unknown unit: '{}'\nvalid units: s, ms, m", u),
      Self::Overflow => "duration value too large".into(),
      Self::Zero => "duration cannot be zero".into(),
    }
  }
}

/// Parse a duration string like "30s", "500ms", "2m" into milliseconds.
fn parse_duration(s: &str) -> std::result::Result<u64, DurationError> {
  let s = s.trim();

  if s.is_empty() {
    return Err(DurationError::Empty);
  }

  // Try to split into number and unit
  let (num_str, unit) = if let Some(idx) = s.find(|c: char| !c.is_ascii_digit()) {
    (&s[..idx], s[idx..].trim())
  } else {
    (s, "") // bare number, default to seconds
  };

  if num_str.is_empty() {
    return Err(DurationError::MissingNumber);
  }

  let num: u64 = num_str
    .parse()
    .map_err(|_| DurationError::InvalidNumber(num_str.to_string()))?;

  let millis = match unit.to_lowercase().as_str() {
    "s" | "sec" | "secs" | "second" | "seconds" => num.checked_mul(MILLIS_PER_SECOND),
    "ms" | "milli" | "millis" | "millisecond" | "milliseconds" => Some(num),
    "m" | "min" | "mins" | "minute" | "minutes" => num.checked_mul(MILLIS_PER_MINUTE),
    "" => num.checked_mul(MILLIS_PER_SECOND), // bare number = seconds
    other => return Err(DurationError::UnknownUnit(other.to_string())),
  };

  let millis = millis.ok_or(DurationError::Overflow)?;

  if millis == 0 {
    return Err(DurationError::Zero);
  }

  Ok(millis)
}

/// Formats milliseconds as a human-readable duration string for display.
/// Returns "Xs" for whole seconds (e.g., "30s"), "Xms" otherwise (e.g., "500ms").
fn format_timeout_display(millis: u64) -> String {
  if millis >= MILLIS_PER_SECOND && millis.is_multiple_of(MILLIS_PER_SECOND) {
    format!("{}s", millis / MILLIS_PER_SECOND)
  } else {
    format!("{}ms", millis)
  }
}

// ============================================================
// Code Generation Helpers
// ============================================================

/// Generates code to spawn a subprocess with timeout handling.
/// The subprocess runs the test binary with the given name and waits for completion
/// or kills it after the timeout expires.
fn quote_subprocess_with_timeout(
  subprocess_extra_args: &TokenStream2,
  millis: u64,
) -> TokenStream2 {
  let timeout_display = format_timeout_display(millis);

  quote! {
    let binary = std::env::args().next().expect("no binary path in args");
    use assay::wait_timeout::ChildExt;
    use std::io::Read;

    let mut child = std::process::Command::new(&binary)
      .arg(&name)
      .arg("--exact")
      #subprocess_extra_args
      .env(#ENV_ASSAY_SPLIT, "1")
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
}

/// Generates code to spawn a subprocess without timeout (blocking wait).
fn quote_subprocess_no_timeout(subprocess_extra_args: &TokenStream2) -> TokenStream2 {
  quote! {
    let binary = std::env::args().next().expect("no binary path in args");
    let out = std::process::Command::new(&binary)
      .arg(&name)
      .arg("--exact")
      #subprocess_extra_args
      .env(#ENV_ASSAY_SPLIT, "1")
      .output()
      .expect("executed a subprocess");
    let stdout = String::from_utf8(out.stdout).unwrap();
  }
}

/// Generates the subprocess handling code based on whether timeout is configured.
fn quote_subprocess_handling(
  subprocess_extra_args: &TokenStream2,
  timeout: Option<u64>,
) -> TokenStream2 {
  match timeout {
    Some(millis) => quote_subprocess_with_timeout(subprocess_extra_args, millis),
    None => quote_subprocess_no_timeout(subprocess_extra_args),
  }
}

/// Generates the retry loop that runs the subprocess and extracts failure output.
/// This handles running the test subprocess, checking for failures, and extracting
/// the relevant stdout section for error reporting.
fn quote_retry_loop(subprocess_handling: &TokenStream2, retry_count: u32) -> TokenStream2 {
  quote! {
    let mut last_failure: Option<String> = None;
    for _attempt in 1..=#retry_count {
      #subprocess_handling
      if stdout.contains(&format!("{name} - should panic ... ok"))
        || stdout.contains(&format!("{name} ... FAILED"))
      {
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
  }
}

/// Generates the full test execution logic including nextest detection,
/// subprocess spawning, retry handling, and failure propagation.
fn quote_test_execution(
  subprocess_handling: &TokenStream2,
  child: &TokenStream2,
  ret: &TokenStream2,
  retry_count: u32,
  test_name_expr: &TokenStream2,
) -> TokenStream2 {
  let retry_loop = quote_retry_loop(subprocess_handling, retry_count);

  quote! {
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
      let name = #test_name_expr;
      if std::env::var(#ENV_ASSAY_SPLIT)
        .as_ref()
        .map(|s| s.as_str() != "1")
        .unwrap_or(true)
      {
        #retry_loop
        #ret
      } else {
        #child
      }
    }
  }
}

// ============================================================
// Attribute Parsing
// ============================================================

/// Parsed representation of all arguments to the `#[assay]` attribute macro.
/// Each field corresponds to a possible attribute argument.
///
/// Note: `#[ignore]` and `#[should_panic]` are standard Rust test attributes
/// and should be placed directly on the function, not inside `#[assay(...)]`.
struct AssayAttribute {
  /// (source_path, optional_dest_path)
  /// If dest is None, file is copied to temp root with its filename only
  include: Option<Vec<(String, Option<String>)>>,
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
  /// Span of the `matrix` keyword for better error messages
  matrix_span: Option<Span>,
}

impl Parse for AssayAttribute {
  fn parse(input: ParseStream) -> Result<Self> {
    let mut include = None;
    let mut env = None;
    let mut setup = None;
    let mut teardown = None;
    let mut timeout = None;
    let mut retries = None;
    let mut cases = None;
    let mut matrix = None;
    let mut matrix_span = None;

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
              "expected `=` here\nhelp: use `include = [\"file.txt\"]`",
            )
          })?;
          let array: ExprArray = input.parse().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected `[...]` here\nhelp: use `include = [\"file.txt\", \"other.txt\"]`",
            )
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
              "expected `=` here\nhelp: use `env = [(\"KEY\", \"value\")]`",
            )
          })?;
          let array: ExprArray = input.parse().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected `[...]` here\nhelp: use `env = [(\"KEY\", \"value\")]`",
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
              format!("expected `=` here\nhelp: use `{} = my_function()`", val),
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
            syn::Error::new(e.span(), "expected `=` here\nhelp: use `timeout = \"30s\"`")
          })?;

          let lit: syn::LitStr = input.parse().map_err(|e| {
            syn::Error::new(
              e.span(),
              "expected duration string here\nhelp: use `timeout = \"30s\"` or `timeout = \"500ms\"`",
            )
          })?;

          let duration_str = lit.value();
          let millis = parse_duration(&duration_str).map_err(|err| {
            syn::Error::new_spanned(
              &lit,
              format!(
                "{}\nhelp: use `timeout = \"30s\"` or `timeout = \"500ms\"`",
                err.message()
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

          input
            .parse::<Token![=]>()
            .map_err(|e| syn::Error::new(e.span(), "expected `=` here\nhelp: use `retries = 3`"))?;

          let lit: syn::LitInt = input.parse().map_err(|e| {
            syn::Error::new(e.span(), "expected number here\nhelp: use `retries = 3`")
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
              "expected `=` here\nhelp: use `cases = [name: (arg1, arg2)]`",
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
                "expected identifier here\nhelp: use `cases = [my_case: (arg1, arg2)]`",
              )
            })?;

            content.parse::<Token![:]>().map_err(|e| {
              syn::Error::new(
                e.span(),
                "expected `:` here\nhelp: use `cases = [my_case: (arg1, arg2)]`",
              )
            })?;

            let args: ExprTuple = content.parse().map_err(|e| {
              syn::Error::new(
                e.span(),
                "expected `(...)` here\nhelp: use `cases = [my_case: (arg1, arg2)]`",
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
              "expected `=` here\nhelp: use `matrix = [param: [val1, val2]]`",
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
                "expected identifier here\nhelp: use `matrix = [param: [val1, val2]]`",
              )
            })?;

            content.parse::<Token![:]>().map_err(|e| {
              syn::Error::new(
                e.span(),
                "expected `:` here\nhelp: use `matrix = [param: [val1, val2]]`",
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
          matrix_span = Some(ident.span());
        }
        unknown => {
          // Check if user is trying to use standard test attributes inside #[assay(...)]
          if matches!(
            unknown,
            "ignore" | "ignored" | "should_panic" | "panic" | "panics"
          ) {
            let attr_name = if unknown.contains("panic") {
              "should_panic"
            } else {
              "ignore"
            };
            return Err(syn::Error::new_spanned(
              &ident,
              format!(
                "`{}` should be a separate attribute, not inside #[assay(...)]\nhelp: use `#[{}]` before or after `#[assay]`",
                unknown, attr_name
              ),
            ));
          }

          let suggestion = match unknown {
            "includes" => Some("include"),
            "envs" | "environment" => Some("env"),
            "set_up" | "before" | "before_each" => Some("setup"),
            "tear_down" | "after" | "after_each" | "cleanup" => Some("teardown"),
            "time" | "time_out" | "timelimit" | "time_limit" => Some("timeout"),
            "retry" | "attempts" | "tries" | "repeat" | "flaky" => Some("retries"),
            "case" | "params" | "parameters" | "test_cases" => Some("cases"),
            "values" | "combinations" | "cartesian" | "parametrize" => Some("matrix"),
            _ => None,
          };

          let valid_attrs = VALID_ATTRIBUTES.join(", ");

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
      env,
      setup,
      teardown,
      timeout,
      retries,
      cases,
      matrix,
      matrix_span,
    })
  }
}

// ============================================================
// Main Macro
// ============================================================

/// A super powered testing macro for Rust.
///
/// The `#[assay]` attribute transforms a function into a test that runs in its own
/// process with automatic temp directory isolation. Each test gets a clean environment
/// with no interference from other tests.
///
/// # Supported Attributes
///
/// - `include = ["file.txt", ("src/a.rs", "dest/a.rs")]` - Copy files into temp directory
/// - `env = [("KEY", "value")]` - Set environment variables
/// - `setup = setup_fn()?` - Run before the test
/// - `teardown = teardown_fn()` - Run after the test
/// - `timeout = "30s"` - Fail if test exceeds duration (supports s, ms, m)
/// - `retries = 3` - Retry on failure up to N times total
/// - `cases = [name: (args)]` - Parameterized testing with named cases
/// - `matrix = [param: [values]]` - Combinatorial testing
///
/// # Standard Test Attributes
///
/// Use standard Rust test attributes as separate attributes:
/// - `#[should_panic]` or `#[should_panic(expected = "...")]`
/// - `#[ignore]`
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

  // Detect standard test attributes from the function
  let has_ignore = func.attrs.iter().any(|attr| attr.path.is_ident("ignore"));
  let has_should_panic = func
    .attrs
    .iter()
    .any(|attr| attr.path.is_ident("should_panic"));

  // Find the should_panic attribute to preserve its arguments (e.g., expected = "...")
  let should_panic_attr = func
    .attrs
    .iter()
    .find(|attr| attr.path.is_ident("should_panic"))
    .cloned();

  // Preserve non-conflicting attributes from the original function
  let other_attrs: Vec<_> = func
    .attrs
    .into_iter()
    .filter(|attr| {
      // Skip attributes that we handle specially
      !attr.path.is_ident("test")
        && !attr.path.is_ident("ignore")
        && !attr.path.is_ident("should_panic")
    })
    .collect();

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

  // Generate attribute tokens for the test function
  let ignore = if has_ignore {
    quote! { #[ignore] }
  } else {
    quote! {}
  };

  // Preserve the full should_panic attribute including any arguments like expected = "..."
  let should_panic = match &should_panic_attr {
    Some(attr) => quote! { #attr },
    None => quote! {},
  };

  let fn_sig = if has_should_panic {
    quote! { #vis #sig }
  } else {
    quote! { #vis #sig -> assay::Result<()> }
  };

  let ret = if has_should_panic {
    quote! {}
  } else {
    quote! { Ok(()) }
  };

  let child = if has_should_panic {
    quote! { child().unwrap() }
  } else {
    quote! { child() }
  };

  // For ignored tests, subprocess needs --ignored flag
  let subprocess_extra_args = if has_ignore {
    quote! { .arg("--ignored") }
  } else {
    quote! {}
  };

  // Generate subprocess handling code - with or without timeout
  let subprocess_handling = quote_subprocess_handling(&subprocess_extra_args, attr.timeout);

  // Get retry count (1 = run once, no retries)
  let retry_count = attr.retries.unwrap_or(1);

  // Return type for generated test functions
  let ret_type = if has_should_panic {
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
    let subprocess_handling_for_test =
      quote_subprocess_handling(&subprocess_extra_args, attr.timeout);
    let test_fn_name_for_body = test_fn_name.clone();

    let test_name_expr = quote! {
      {
        let mut module = module_path!()
          .split("::")
          .into_iter()
          .skip(1)
          .collect::<Vec<_>>();
        module.push(stringify!(#test_fn_name_for_body));
        module.join("::")
      }
    };

    let test_execution = quote_test_execution(
      &subprocess_handling_for_test,
      &child,
      &ret,
      retry_count,
      &test_name_expr,
    );

    quote! {
      #(#other_attrs)*
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

        #test_execution
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
      let span = attr.matrix_span.unwrap_or_else(|| sig.span());
      return syn::Error::new(
        span,
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
    let test_name_expr = quote! {
      {
        let mut module = module_path!()
          .split("::")
          .into_iter()
          .skip(1)
          .collect::<Vec<_>>();
        module.push(stringify!(#name));
        module.join("::")
      }
    };

    let test_execution = quote_test_execution(
      &subprocess_handling,
      &child,
      &ret,
      retry_count,
      &test_name_expr,
    );

    let expanded = quote! {
      #(#other_attrs)*
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

        #test_execution
      }
    };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
  }
}
