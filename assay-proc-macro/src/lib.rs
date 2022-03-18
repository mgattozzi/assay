/*
 * Copyright (C) 2021 Michael Gattozzi <self@mgattozzi.dev>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use proc_macro::TokenStream;
use quote::quote;
use syn::{
  parse::{Parse, ParseStream},
  parse_macro_input, Expr, ExprArray, ExprLit, ExprTuple, Ident, ItemFn, Lit, Result, Token,
};

struct AssayAttribute {
  include: Option<Vec<String>>,
  should_panic: bool,
  env: Option<Vec<(String, String)>>,
  setup: Option<Expr>,
  teardown: Option<Expr>,
}

impl Parse for AssayAttribute {
  fn parse(input: ParseStream) -> Result<Self> {
    let mut include = None;
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
          let _: Token![=] = input.parse()?;
          let array: ExprArray = input.parse()?;
          include = Some(
            array
              .elems
              .into_iter()
              .filter_map(|e| match e {
                Expr::Lit(ExprLit {
                  lit: Lit::Str(lit_str),
                  ..
                }) => Some(lit_str.value()),
                _ => None,
              })
              .collect(),
          );
        }
        "should_panic" => should_panic = true,
        "env" => {
          let _: Token![=] = input.parse()?;
          let array: ExprArray = input.parse()?;
          env = Some(
            array
              .elems
              .into_iter()
              .filter_map(|e| match e {
                Expr::Tuple(ExprTuple { elems, .. }) => match (&elems[0], &elems[1]) {
                  (
                    Expr::Lit(ExprLit {
                      lit: Lit::Str(lit_1),
                      ..
                    }),
                    Expr::Lit(ExprLit {
                      lit: Lit::Str(lit_2),
                      ..
                    }),
                  ) => Some((lit_1.value(), lit_2.value())),
                  _ => None,
                },
                _ => None,
              })
              .collect(),
          );
        }
        val @ "setup" | val @ "teardown" => {
          let _: Token![=] = input.parse()?;
          let x = input.parse()?;
          if val == "setup" {
            setup = Some(x);
          } else {
            teardown = Some(x);
          }
        }
        _ => {}
      }
    }

    Ok(AssayAttribute {
      include,
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
      async fn inner_async() -> Result<(), Box<dyn std::error::Error>> {
        #block
        Ok(())
      }
      assay::async_runtime::Runtime::block_on(inner_async())??;
    }
  } else {
    quote! { #block }
  };

  let expanded = quote! {
      #[test]
      #should_panic
      #vis #sig {
        fn modify(_: &mut std::process::Command) {}

        fn parent(child: &mut assay::ChildWrapper, _: &mut std::fs::File) {
          let child = child.wait().unwrap();
          if !child.success() {
              panic!("Assay test failed")
          }
        }

        fn child() {
          #[allow(unreachable_code)]
          if let Err(e) = || -> Result<(), Box<dyn std::error::Error>> {
            use assay::{assert_eq, assert_eq_sorted, assert_ne};
            #include
            #setup
            #env
            #body
            #teardown
            Ok(())
          }() {
            panic!("Error: {}", e);
          }
        }

      if std::env::var("NEXTEST_EXECUTION_MODE")
        .ok()
        .as_ref()
        .map(|s| s.as_str() == "process-per-test")
        .unwrap_or(false)
      {
        child();
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

        assay::fork(
            &name,
            assay::rusty_fork_id!(),
            modify,
            parent,
            child
        ).expect("We forked the test using assay");
      }
    }
  };

  // Hand the output tokens back to the compiler.
  TokenStream::from(expanded)
}
