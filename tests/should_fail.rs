/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Inverted tests that we *want* to panic or pass. Given assay causes a test
//! to spawn itself we need to make sure these tests can actually properly fail.
//! We run these with a test that calls cargo test --ignored. This lets us run
//! them in CI with cargo test --workspace to make sure what would fail is
//! tested without failing cargo test. These tests are all ignored by default
//! and must be explicitly called for if we want them to run.

use assay::assay;
use std::process::Command;

#[assay(ignore)]
fn should_panic_and_cause_a_failure_case() {
  panic!()
}

#[assay(ignore, should_panic)]
fn should_not_panic_and_cause_a_failure_case() {}

#[assay(ignore, timeout = "2s")]
fn timeout_exceeded() {
  // Sleep longer than timeout - this SHOULD fail with timeout
  std::thread::sleep(std::time::Duration::from_secs(10));
}

#[test]
fn panics_in_macros() {
  let output = Command::new("cargo")
    .args([
      "test",
      "--workspace",
      "--",
      "--ignored",
      "panic_and_cause_a_failure_case",
    ])
    .env_remove("NEXTEST_EXECUTION_MODE")
    .output()
    .unwrap();
  let tests = String::from_utf8(output.stdout).unwrap();

  // Check that the expected failure cases are present in the output
  // Note: Rust's panic output format varies by version (thread IDs, etc.)
  let has_not_panic_failure = tests.contains("should_not_panic_and_cause_a_failure_case")
    && tests.contains("note: test did not panic as expected");
  let has_panic_failure = tests.contains("should_panic_and_cause_a_failure_case")
    && tests.contains("panicked at")
    && tests.contains("explicit panic");

  if !has_not_panic_failure && !has_panic_failure {
    panic!("Unexpected output for panics.\n\nOutput:\n{}", tests);
  }
}

#[test]
fn timeout_fires_correctly() {
  let output = Command::new("cargo")
    .args(["test", "--workspace", "--", "--ignored", "timeout_exceeded"])
    .env_remove("NEXTEST_EXECUTION_MODE")
    .output()
    .unwrap();
  let tests = String::from_utf8(output.stdout).unwrap();

  // Verify the test timed out
  let has_timeout_failure =
    tests.contains("timeout_exceeded") && tests.contains("test timed out after 2s");

  if !has_timeout_failure {
    panic!("Expected timeout failure not found.\n\nOutput:\n{}", tests);
  }
}
