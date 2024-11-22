/*
 * Copyright (C) 2021 Michael Gattozzi <self@mgattozzi.dev>
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
    .output()
    .unwrap();
  let tests = String::from_utf8(output.stdout).unwrap();

  if !tests.contains(
    "---- should_not_panic_and_cause_a_failure_case stdout ----
note: test did not panic as expected",
  ) && !tests.contains(
    "---- should_panic_and_cause_a_failure_case stdout ----
thread 'should_panic_and_cause_a_failure_case' panicked at tests/should_fail.rs:21:3:
explicit panic
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
  ) {
    panic!("Unexpected output for panics.\n\nOutput:\n{}", tests);
  }
}
