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

struct AssayAttribute {
  include: Option<Vec<String>>,
  ignore: bool,
  should_panic: bool,
  env: Option<Vec<(String, String)>>,
  setup: Option<Expr>,
  teardown: Option<Expr>,
}

impl Parse for AssayAttribute {
  fn parse(input: ParseStream) -> Result<Self> {
    let mut include = None;
    let mut ignore = false;
    let mut should_panic = false;
    let mut env = None;
    let mut setup = None;
    let mut teardown = None;

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
              Expr::Lit(ExprLit {
                lit: Lit::Str(lit_str),
                ..
              }) => {
                files.push(lit_str.value());
              }
              other => {
                return Err(syn::Error::new_spanned(
                  &other,
                  "include array elements must be string literals\nhelp: use `include = [\"file1.txt\", \"file2.txt\"]`"
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
        unknown => {
          let suggestion = match unknown {
            "includes" => Some("include"),
            "envs" | "environment" => Some("env"),
            "panic" | "panics" => Some("should_panic"),
            "ignored" => Some("ignore"),
            "set_up" | "before" | "before_each" => Some("setup"),
            "tear_down" | "after" | "after_each" | "cleanup" => Some("teardown"),
            _ => None,
          };

          let valid_attrs = "include, ignore, should_panic, env, setup, teardown";

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
    for file in include {
      out = quote! {
        #out
        fs.include(#file)?;
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
          let mut args = std::env::args().collect::<Vec<String>>();
          if !args.contains(&name) {
            args.push(name.clone());
          }
          let out = std::process::Command::new(&args[0])
            .args(if args.len() == 1 { &[] } else { &args[1..] })
            .env("ASSAY_SPLIT", "1")
            .output()
            .expect("executed a subprocess");
          let stdout = String::from_utf8(out.stdout).unwrap();
          if stdout.contains(&format!("{name} - should panic ... ok")) || stdout.contains(&format!("{name} ... FAILED")) {
            let stdout_line = format!("---- {name} stdout ----");
            let split = stdout
              .lines()
              .skip_while(|line| line != &stdout_line)
              .skip(1)
              .take_while(|s| !s.starts_with("----") && !s.starts_with("failures:"))
              .collect::<Vec<&str>>()
              .join("\n");
            assay::panic_replace();
            panic!("ASSAY_PANIC_INTERNAL_MESSAGE\n{split}")
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
