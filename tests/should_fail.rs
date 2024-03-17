/*
 * Copyright (C) 2021 Michael Gattozzi <self@mgattozzi.dev>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Inverted tests that we *want* to panic or pass. Given assay causes a test
//! to spawn itself we need to make sure these tests can actually properly fail.
//! We run these as part of a script to actually call them. Mainly because these
//! test that the assay macro works and if a test fails or does not it works as
//! expected. These are all ignored by default and must be explicitly called for
//! if we want them to run. This is because we expect these to fail and so we need
//! to run commands to check for a failure output for CI purposes

use assay::assay;

#[assay(ignore)]
fn should_panic_and_cause_a_failure_case() {
  panic!()
}

#[assay(ignore, should_panic)]
fn should_not_panic_and_cause_a_failure_case() {}
