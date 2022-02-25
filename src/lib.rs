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
pub use pretty_assertions::{assert_eq, assert_ne};
#[doc(hidden)]
pub use rusty_fork::{fork, rusty_fork_id, rusty_fork_test_name, ChildWrapper};
#[doc(hidden)]
pub use tokio::runtime::Runtime;

use std::{
  env,
  error::Error,
  fs::{copy, create_dir_all},
  path::{Component, Path, PathBuf},
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

  pub fn include(&self, path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    // Get our pathbuf to the file to include
    let mut inner_path = path.as_ref().to_owned();

    // If the path given is not absolute then it's relative to the dir we
    // ran the test from
    let is_relative = inner_path.is_relative();
    if is_relative {
      inner_path = self.ran_from.join(&path);
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
      path.as_ref().into()
    };

    // If the relative path to the file includes parent directories create
    // them
    if let Some(parent) = relative.parent() {
      create_dir_all(dir.join(parent))?;
    }

    // Copy the file over from the file system into the temp file system
    copy(inner_path, dir.join(relative))?;

    Ok(())
  }
}
