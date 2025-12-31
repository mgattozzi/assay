/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use assay::assay;
use std::{
  collections::HashMap,
  env, fs,
  future::Future,
  path::PathBuf,
  pin::Pin,
  task::{Context, Poll},
};

#[assay]
fn private_1() {
  fs::write("test", "This is a test\nprivate 1\n").unwrap();
  assert_eq!(
    "This is a test\nprivate 1\n",
    &fs::read_to_string("test").unwrap()
  );
}

#[assay]
fn private_2() {
  fs::write("test", "This is a test\nprivate 2\n")?;
  assert_eq!("This is a test\nprivate 2\n", &fs::read_to_string("test")?);
}

#[assay(include = ["Cargo.toml", "src/lib.rs"])]
fn include() {
  assert!(fs::metadata("lib.rs")?.is_file());
  assert!(fs::metadata("Cargo.toml")?.is_file());
  assert!(!PathBuf::from("src/lib.rs").exists());
}

#[assay(include = [("Cargo.toml", "config/Cargo.toml"), ("src/lib.rs", "sources/lib.rs")])]
fn include_with_custom_dest() {
  assert!(fs::metadata("config/Cargo.toml")?.is_file());
  assert!(fs::metadata("sources/lib.rs")?.is_file());
  assert!(!PathBuf::from("Cargo.toml").exists());
  assert!(!PathBuf::from("lib.rs").exists());
}

#[assay(include = ["Cargo.toml", ("src/lib.rs", "custom/lib.rs")])]
fn include_mixed_syntax() {
  assert!(fs::metadata("Cargo.toml")?.is_file());
  assert!(fs::metadata("custom/lib.rs")?.is_file());
}

#[assay(should_panic)]
fn hash_map_comparison() {
  let map1: HashMap<String, u8> = (0..5).map(|n| (n.to_string(), n)).collect();
  let mut map2: HashMap<String, u8> = (0..5).map(|n| (n.to_string(), n)).collect();
  // Force the last item to be different
  map2.insert("4".to_string(), 5);
  assert_eq_sorted!(map1, map2);
}

#[assay]
async fn async_func() {
  ReadyOnPoll.await;
}

#[assay(should_panic)]
fn panic_test() {
  panic!("Panic! At The Proc-Macro");
}

#[assay(include = ["Cargo.toml"], should_panic)]
fn multiple_attribute_values() {
  panic!("Panic! At The Proc-Macro 2: Cargo.toml Boogaloo");
}

#[assay(should_panic, include = ["Cargo.toml"])]
fn multiple_attribute_values_in_different_order() {
  panic!("Panic! At The Proc-Macro 3: Attribute Switcharoo");
}

#[assay(
  env = [
    ("GOODBOY", "Bukka"),
    ("BADDOGS", "false")
  ]
)]
fn env_vars() {
  assert_eq!(env::var("GOODBOY")?, "Bukka");
  assert_eq!(env::var("BADDOGS")?, "false");
}

#[assay(
  setup = setup_func(5)?,
  teardown = teardown_func(),
)]
fn setup_teardown_test_1() {
  assert_eq!(fs::read_to_string("setup")?, "Value: 5");
}

#[assay(
  setup = setup_func_2(),
  teardown = teardown_func(),
)]
fn setup_teardown_test_2() {
  assert_eq!(fs::read_to_string("setup")?, "Value: 5");
}

#[assay(
  setup = setup_func_2(),
  include = ["Cargo.toml", "src/lib.rs"],
  env = [
    ("GOODBOY", "Bukka"),
    ("BADDOGS", "false")
  ],
  teardown = teardown_func(),
  should_panic,
)]
async fn one_test_to_call_it_all() {
  ReadyOnPoll.await;

  assert_eq!(env::var("GOODBOY")?, "Bukka");
  assert_eq!(env::var("BADDOGS")?, "false");
  assert_eq!(fs::read_to_string("setup")?, "Value: 5");
  assert!(PathBuf::from("Cargo.toml").exists());
  assert!(PathBuf::from("lib.rs").exists());

  // Removing this actually causes the test to fail
  panic!();
}

#[assay(
  setup = setup_func(5)?,
  env = [
    ("GOODBOY", "Bukka"),
    ("BADDOGS", "false")
  ],
  teardown = teardown_func(),
  include = ["Cargo.toml", "src/lib.rs"],
  should_panic,
)]
async fn one_test_to_call_it_all_2() {
  ReadyOnPoll.await;

  assert_eq!(env::var("GOODBOY")?, "Bukka");
  assert_eq!(env::var("BADDOGS")?, "false");
  assert_eq!(fs::read_to_string("setup")?, "Value: 5");
  assert!(PathBuf::from("Cargo.toml").exists());
  assert!(PathBuf::from("lib.rs").exists());

  // Removing this actually causes the test to fail
  panic!();
}

// Timeout tests
#[assay(timeout = "5s")]
fn timeout_passes() {
  std::thread::sleep(std::time::Duration::from_millis(100));
}

#[assay(timeout = "500ms")]
fn timeout_millis_passes() {
  std::thread::sleep(std::time::Duration::from_millis(50));
}

#[assay(timeout = "5s")]
async fn async_timeout_passes() {
  ReadyOnPoll.await;
}

#[assay(
  timeout = "10s",
  env = [("TIMEOUT_TEST_VAR", "value")],
  include = ["Cargo.toml"],
)]
fn timeout_with_other_features() {
  assert_eq!(env::var("TIMEOUT_TEST_VAR").unwrap(), "value");
  assert!(PathBuf::from("Cargo.toml").exists());
}

// Retries tests
#[assay(retries = 3)]
fn retries_passes_immediately() {
  assert_eq!(1 + 1, 2);
}

#[assay(retries = 2)]
fn retries_with_single_retry() {
  assert!(true);
}

#[assay(retries = 2)]
async fn async_retries_test() {
  ReadyOnPoll.await;
}

#[assay(
  retries = 3,
  timeout = "10s",
  env = [("RETRIES_TEST_VAR", "value")],
  include = ["Cargo.toml"],
)]
fn retries_with_other_features() {
  assert_eq!(env::var("RETRIES_TEST_VAR").unwrap(), "value");
  assert!(PathBuf::from("Cargo.toml").exists());
}

fn setup_func(input: i32) -> assay::Result<()> {
  fs::write("setup", format!("Value: {}", input))?;
  Ok(())
}

fn teardown_func() {
  fs::remove_file("setup").unwrap();
  assert!(!PathBuf::from("setup").exists());
}

fn setup_func_2() {
  fs::write("setup", "Value: 5").unwrap();
}

struct ReadyOnPoll;
impl Future for ReadyOnPoll {
  type Output = ();
  fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
    Poll::Ready(())
  }
}

// Parameterized tests - cases
#[assay(
  cases = [
    positive: (2, 3, 5),
    zeros: (0, 0, 0),
    negative: (-1, -2, -3),
  ]
)]
fn cases_basic_addition(a: i32, b: i32, expected: i32) {
  assert_eq!(a + b, expected);
}

#[assay(
  cases = [
    hello: ("hello", 5),
    empty: ("", 0),
    spaces: ("a b c", 5),
  ]
)]
fn cases_string_length(s: &str, expected: usize) {
  assert_eq!(s.len(), expected);
}

#[assay(
  cases = [
    case_true: (true, 1),
    case_false: (false, 0),
  ]
)]
fn cases_two_params(b: bool, n: i32) {
  if b {
    assert_eq!(n, 1);
  } else {
    assert_eq!(n, 0);
  }
}

#[assay(
  cases = [
    with_file: (true, "Cargo.toml"),
  ],
  include = ["Cargo.toml"],
)]
fn cases_with_include(check_file: bool, filename: &str) {
  if check_file {
    assert!(PathBuf::from(filename).exists());
  }
}

// Parameterized tests - matrix (combinatorial)
#[assay(
  matrix = [
    a: [1, 2],
    b: [10, 20],
  ]
)]
fn matrix_basic(a: i32, b: i32) {
  assert!(a * b >= 10);
}

#[assay(
  matrix = [
    x: [true, false],
    y: [true, false],
  ]
)]
fn matrix_booleans(x: bool, y: bool) {
  // Test all 4 combinations
  let _ = x && y;
  let _ = x || y;
}

#[assay(
  matrix = [
    s: ["hello", "world"],
    n: [1, 2, 3],
  ]
)]
fn matrix_mixed_types(s: &str, n: i32) {
  assert!(!s.is_empty());
  assert!(n > 0);
}

#[assay(
  matrix = [
    val: [-1, 0, 1],
    mult: [1, 2],
  ]
)]
fn matrix_two_params(val: i32, mult: i32) {
  let result = val * mult;
  assert!(result >= -2 && result <= 2);
}

#[assay(
  matrix = [
    a: [1, 2],
    b: [3, 4],
  ],
  timeout = "5s",
)]
fn matrix_with_timeout(a: i32, b: i32) {
  assert!(a + b > 0);
}
