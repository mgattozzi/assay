/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use assay::assay;
use assay::eyre::bail;
use std::error::Error;
use std::fmt::Display;
use std::process::{Command, Stdio};

#[assay(ignore)]
fn result_output_test() {
  #[derive(Debug)]
  struct TestError;

  impl Error for TestError {}
  impl Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      write!(f, "This test failed")
    }
  }

  return Err(TestError.into());
}

#[assay(ignore)]
fn result_bail_test() {
  bail!("This is a test failure");
}

#[test]
fn check_result_output() {
  let output = Command::new("cargo")
    .args([
      "test",
      "--workspace",
      "--",
      "--ignored",
      "result_output_test",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .unwrap();
  let check_result = String::from_utf8(output.stdout).unwrap();
  let compare = [
    "---- result_output_test stdout ----",
    "Error: This test failed",
    "Location:",
    "assay_result.rs:27:24",
  ];
  for line in compare {
    if !check_result.contains(line) {
      panic!("Unexpected output for assertions.\n\nOutput:\n{check_result}");
    }
  }
}

#[test]
fn check_bail_output() {
  let output = Command::new("cargo")
    .args(["test", "--workspace", "--", "--ignored", "result_bail_test"])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .unwrap();
  let check_result = String::from_utf8(output.stdout).unwrap();
  let compare = [
    "---- result_bail_test stdout ----",
    "Error: This is a test failure",
    "Location:",
    "assay_result.rs:32:3",
  ];
  for line in compare {
    if !check_result.contains(line) {
      panic!("Unexpected output for assertions.\n\nOutput:\n{check_result}");
    }
  }
}
