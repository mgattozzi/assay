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
pub use wait_timeout;

use std::{
  env,
  fs::{copy, create_dir_all},
  panic,
  path::{Path, PathBuf},
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

  /// Include a file in the test's temporary directory.
  ///
  /// The file is copied to the root of the temp directory using only its filename.
  /// For example, `include("src/fixtures/data.json")` copies to `<temp>/data.json`.
  ///
  /// To specify a custom destination path, use [`include_as`](Self::include_as).
  pub fn include(&self, source: impl AsRef<Path>) -> Result<()> {
    let source = source.as_ref();

    // Extract just the filename for the destination
    let filename = source.file_name().ok_or_else(|| {
      eyre::eyre!(
        "cannot include '{}': path has no filename",
        source.display()
      )
    })?;

    self.include_as(source, filename)
  }

  /// Include a file in the test's temporary directory at a custom destination path.
  ///
  /// # Examples
  ///
  /// ```ignore
  /// // Copy src/fixtures/data.json to <temp>/config/data.json
  /// fs.include_as("src/fixtures/data.json", "config/data.json")?;
  /// ```
  pub fn include_as(&self, source: impl AsRef<Path>, dest: impl AsRef<Path>) -> Result<()> {
    let source = source.as_ref();
    let dest = dest.as_ref();

    // Resolve source to absolute path if relative
    let abs_source = if source.is_relative() {
      self.ran_from.join(source)
    } else {
      source.to_owned()
    };

    // Validate source file exists
    if !abs_source.exists() {
      return Err(eyre::eyre!(
        "cannot include '{}': file not found\nsearched at: {}",
        source.display(),
        abs_source.display()
      ));
    }

    if !abs_source.is_file() {
      return Err(eyre::eyre!(
        "cannot include '{}': path is not a file (is it a directory?)\npath: {}",
        source.display(),
        abs_source.display()
      ));
    }

    let full_dest = self.directory.path().join(dest);

    if let Some(parent) = full_dest.parent() {
      create_dir_all(parent).wrap_err_with(|| {
        format!(
          "failed to create directory structure for '{}'\ntarget directory: {}",
          dest.display(),
          parent.display()
        )
      })?;
    }

    copy(&abs_source, &full_dest).wrap_err_with(|| {
      format!(
        "failed to copy '{}' to test directory\nsource: {}\ndestination: {}",
        source.display(),
        abs_source.display(),
        full_dest.display()
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
