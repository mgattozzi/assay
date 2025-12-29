/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

pub mod net;

pub use assay_proc_macro::assay;
pub use eyre;
use eyre::WrapErr;
pub use pretty_assertions_sorted::{assert_eq, assert_eq_sorted, assert_ne};

use std::{
  env,
  fs::{copy, create_dir_all},
  panic,
  path::{Component, Path, PathBuf},
  sync::OnceLock,
};
use tempfile::{Builder, TempDir};

pub type Result<T> = std::result::Result<T, eyre::Report>;

#[doc(hidden)]
pub static PANIC_HOOK_REPLACE: OnceLock<()> = OnceLock::new();
#[doc(hidden)]
pub fn panic_replace() {
  const HEADER: &str = "ASSAY_PANIC_INTERNAL_MESSAGE\n";
  PANIC_HOOK_REPLACE.get_or_init(|| {
    let default = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
      let msg = panic_info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| {
          panic_info
            .payload()
            .downcast_ref::<String>()
            .map(|s| s.to_owned())
        })
        .unwrap_or_default();
      if let Some(message) = msg.strip_prefix(HEADER) {
        println!("{}", message.trim());
      } else {
        default(panic_info);
      }
    }))
  });
}

#[doc(hidden)]
pub struct PrivateFS {
  ran_from: PathBuf,
  directory: TempDir,
}

impl PrivateFS {
  pub fn new() -> Result<Self> {
    let ran_from =
      env::current_dir().wrap_err("failed to get current directory for test isolation")?;
    let directory = Builder::new()
      .prefix("private")
      .tempdir()
      .wrap_err("failed to create temporary directory for test isolation")?;
    env::set_current_dir(directory.path()).wrap_err_with(|| {
      format!(
        "failed to change to temporary directory: {}",
        directory.path().display()
      )
    })?;
    Ok(Self {
      ran_from,
      directory,
    })
  }

  pub fn include(&self, path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();

    // Get our pathbuf to the file to include
    let mut inner_path = path.to_owned();

    // If the path given is not absolute then it's relative to the dir we
    // ran the test from
    let is_relative = inner_path.is_relative();
    if is_relative {
      inner_path = self.ran_from.join(path);
    }

    // Validate source file exists before attempting copy
    if !inner_path.exists() {
      return Err(eyre::eyre!(
        "cannot include '{}': file not found\nsearched at: {}",
        path.display(),
        inner_path.display()
      ));
    }

    if !inner_path.is_file() {
      return Err(eyre::eyre!(
        "cannot include '{}': path is not a file (is it a directory?)\npath: {}",
        path.display(),
        inner_path.display()
      ));
    }

    // Get our working directory
    let dir = self.directory.path().to_owned();

    // Make the relative path of the file in relation to our temp file
    // system based on if it was absolute or not
    let relative = if !is_relative {
      inner_path
        .components()
        .filter(|c| *c != Component::RootDir)
        .collect::<PathBuf>()
    } else {
      path.into()
    };

    // If the relative path to the file includes parent directories create
    // them
    if let Some(parent) = relative.parent() {
      let parent_path = dir.join(parent);
      create_dir_all(&parent_path).wrap_err_with(|| {
        format!(
          "failed to create directory structure for '{}'\ntarget directory: {}",
          path.display(),
          parent_path.display()
        )
      })?;
    }

    // Copy the file over from the file system into the temp file system
    let dest = dir.join(&relative);
    copy(&inner_path, &dest).wrap_err_with(|| {
      format!(
        "failed to copy '{}' to test directory\nsource: {}\ndestination: {}",
        path.display(),
        inner_path.display(),
        dest.display()
      )
    })?;

    Ok(())
  }
}

// Async functionality
#[doc(hidden)]
#[cfg(any(feature = "async-tokio-runtime", feature = "async-std-runtime"))]
pub mod async_runtime {
  use super::Result;
  use std::future::Future;
  pub struct Runtime;
  impl Runtime {
    #[cfg(feature = "async-tokio-runtime")]
    pub fn block_on<F: Future>(fut: F) -> Result<F::Output> {
      Ok(tokio::runtime::Runtime::new()?.block_on(fut))
    }
    #[cfg(feature = "async-std-runtime")]
    pub fn block_on<F: Future>(fut: F) -> Result<F::Output> {
      Ok(async_std::task::block_on(fut))
    }
  }
}
