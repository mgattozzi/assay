/*
 * Copyright (C) 2021 Michael Gattozzi <self@mgattozzi.dev>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! > as·say /ˈaˌsā,aˈsā/ noun - the testing of a metal or ore to determine its ingredients and quality.
//!
//! `assay` is a super powered testing macro for Rust. It lets you run test in
//! parallel while also being their own process so that you can set env vars, or
//! do other per process kinds of settings without interfering with each other,
//! auto mounting and changing to a tempdir, including files in it, choosing
//! setup and tear down functions, async tests, and more!
//!
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/HOW_TO_USE.md"))]

pub use assay_proc_macro::assay;
#[doc(hidden)]
pub use pretty_assertions_sorted::{assert_eq, assert_eq_sorted, assert_ne};
#[doc(hidden)]
pub use rusty_fork::{fork, rusty_fork_id, rusty_fork_test_name, ChildWrapper};

use std::fs::create_dir_all;
use std::{
  env,
  error::Error,
  fs::copy,
  path::{Path, PathBuf},
};
use tempfile::{Builder, TempDir};

#[doc(hidden)]
pub struct PrivateFS {
  ran_from: PathBuf,
  directory: TempDir,
}

impl PrivateFS {
  pub fn new() -> Result<Self, Box<dyn Error>> {
    let ran_from = env::current_dir()?;
    let directory = Builder::new().prefix("private").tempdir()?;
    env::set_current_dir(directory.path())?;
    Ok(Self {
      ran_from,
      directory,
    })
  }

  pub fn include<S, D>(
    &self,
    source_path: S,
    destination_path: Option<D>,
  ) -> Result<(), Box<dyn Error>>
  where
    S: AsRef<Path>,
    D: AsRef<Path>,
  {
    // Get our pathbuf to the file to include
    let mut inner_path = source_path.as_ref().to_owned();

    // If the path given is not absolute then it's relative to the dir we
    // ran the test from
    let is_relative = inner_path.is_relative();
    if is_relative {
      inner_path = self.ran_from.join(&source_path);
    }

    if !inner_path.is_file() {
      panic!(
        "The source path passed to `#[include()]` must point to a file. {:?} is not a file.",
        inner_path
      );
    }

    // Get our working directory
    let dir = self.directory.path().to_owned();

    let destination_path = match destination_path {
      None => {
        // If the destination path is unspecified, we mount the file in the root directory
        // of the test's private filesystem
        match inner_path.file_name() {
          Some(filename) => dir.join(filename),
          None => {
            panic!(
              "Failed to extract the filename from the source path, {:?}.",
              inner_path
            )
          }
        }
      }
      Some(p) => {
        let p = p.as_ref();
        if !p.is_relative() {
          panic!(
            "The destination path for included files must be a relative path. {:?} isn't.",
            p
          );
        }
        // If the relative path to the file includes parent directories create
        // them
        if let Some(parent) = p.parent() {
          create_dir_all(dir.join(parent))?;
        }
        dir.join(p)
      }
    };

    // Copy the file over from the file system into the temp file system
    copy(inner_path, destination_path)?;

    Ok(())
  }
}

// Async functionality
#[doc(hidden)]
#[cfg(any(feature = "async-tokio-runtime", feature = "async-std-runtime"))]
pub mod async_runtime {
  use std::{error::Error, future::Future};
  pub struct Runtime;
  impl Runtime {
    #[cfg(feature = "async-tokio-runtime")]
    pub fn block_on<F: Future>(fut: F) -> Result<F::Output, Box<dyn Error>> {
      Ok(tokio::runtime::Runtime::new()?.block_on(fut))
    }
    #[cfg(feature = "async-std-runtime")]
    pub fn block_on<F: Future>(fut: F) -> Result<F::Output, Box<dyn Error>> {
      Ok(async_std::task::block_on(fut))
    }
  }
}
