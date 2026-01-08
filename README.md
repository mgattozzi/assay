# assay - A super powered testing macro for Rust

> as·say /ˈaˌsā,aˈsā/ noun - the testing of a metal or ore to determine its ingredients and quality.

`assay` is a super powered testing macro for Rust. It lets you run tests in
parallel while also being their own process so that you can set env vars, or
do other per process kinds of settings without interfering with each other,
auto mounting and changing to a tempdir, including files in it, choosing
setup and tear down functions, async tests, and more!

Rust is great, but the testing leaves much to be desired sometimes. With custom
test frameworks being unstable and only an eRFC since 2018 there's not much we
can do to expand the abilities of our tests right? Well that's where `assay`
enters the picture. It seeks to solve a few problems when testing in rust:

- Tests all run in the same process which means setting env vars or changing the
  working dir affects all of the tests meaning you have to resort to things like
  `cargo test -- --test-threads=1` or using some kind of mutex whereby you lose
  the parallelization of running the test suite
- Setting up a temporary file system to run things in for a test and having the
  test run inside it is a pain to setup and being relative to it by using
  `std::env::set_working_dir` is prone to the above issues
- Including fixtures in a test, let alone multiple, can get a bit verbose
- Setting up and tearing down the same thing for each test can be a lot
- Want to run `async` tests? There's no runtime and you have to do setup just to
  run it.
- Using `?` in your test means putting `-> Result<(), Box<dyn std::error::Error>>`
  on every test and it can be tedious
- `assert_eq`/`assert_ne` output can be hard to grok and see why something is
  equal/not equal

`assay` fixes these issues by:

- Running each test as it's own process for you automatically if you use `cargo
  test` or if you use `cargo nextest` then it let's that handle the processes
  being in parallel in their own process for you. This means you can mutate per
  process state as much as you want without affecting other tests and always
  have your tests run in parallel
- Setting per process env vars
- Setting up a temporary directory that the test runs in (sort of like `chroot`
  without the jail aspect and no need for `sudo` privileges)
- Including files you want into the temp directory by specifying them
- Letting you run async tests by simply adding `async` to the test function
- Making all of your tests act as if they returned
  `Result<(), Box<dyn std::error::Error>>`. Use the `?` to your hearts content
  and no need to add the Eye of Sauron (`Ok(())`) to each test
- Automatically importing the `pretty_assertions_sorted` crate so that you can have
  pretty output for `assert_eq`, `assert_eq_sorted`, and `assert_ne`.
- Allowing you to define setup and teardown functions to call for the test

`assay` was born out of personal frustration with the way things are and wanting
to handle the boilerplate without needing to write a whole test framework, while
also pushing the bounds of what we could have today on stable Rust.

# How to use `assay`

You can get started using `assay` by importing the crate into your `Cargo.toml`'s dev
dependencies:

```toml
[dev-dependencies]
assay = "0.1.0"
```

Then importing the macro for your tests:

```
#[cfg(test)]
use assay::assay;
```

## Basic Usage & Automatic Niceties

Just putting on the `#[assay]` attribute is the easiest way to get started:

```rust
use assay::assay;

#[assay]
fn basic_usage() {
  fs::write("test", "This is a test")?;
  assert_eq!(
    "This is a test",
    &fs::read_to_string("test")?
  );
}
```

This does a few things:
- Your test is run in a new process so that it does not have env vars or global
  state changed between tests. This works with both `cargo nextest` and `cargo test`
  where we fork a new process with the default `cargo test` or if you use
  `cargo nextest` then it's already run in parallel as it's own process!
- Is mounted in a temp directory automatically. The above example writes into
  that directory and it's all removed on test completion.
- Allows you to use the `?` operator inside of tests by using the catch all
  `Result<(), Box<dyn std::error::Error>>` return value and it handles adding
  the `Ok(())` value so you don't need to worry about that either.

This alone is great start but there's more!

## Env Vars
You can set environment variables for each test individually. Useful if say you
want to test output at different log levels. The other nice thing is that since
these run as separate process you won't have race conditions in your test from
when they are set and when you read them!

```rust
use assay::assay;

#[assay(
  env = [
    ("RUST_LOG", "debug"),
    ("OTHER", "value")
  ]
)]
fn debug_level() {
  assert_eq!(env::var("RUST_LOG")?, "debug");
  assert_eq!(env::var("OTHER")?, "value");
}

#[assay(
  env = [
    ("RUST_LOG", "warn"),
    ("OTHER", "value")
  ]
)]
fn warn_level() {
  assert_eq!(env::var("RUST_LOG")?, "warn");
  assert_eq!(env::var("OTHER")?, "value");
}
```

## Include files
Sometimes you want to include files in your tests and generating them is one
way, but having it in your version control system and then having them be in
your tests can also be nice! With the `include` directive you can include files
in your test's directory when you start running it.

By default, files are copied to the root of the temp directory using just their
filename:

```rust
use assay::assay;

#[assay(include = ["Cargo.toml", "src/lib.rs"])]
fn include() {
  // Files are at the temp root with the filename only
  assert!(fs::metadata("lib.rs")?.is_file());     // NOT src/lib.rs
  assert!(fs::metadata("Cargo.toml")?.is_file());
}
```

If you need to place files at a specific path within the temp directory, use the
tuple syntax `(source, destination)`:

```rust
use assay::assay;

#[assay(include = [
  ("src/fixtures/data.json", "config/data.json"),
  ("test_data/input.txt", "input.txt"),
])]
fn include_with_paths() {
  assert!(fs::metadata("config/data.json")?.is_file());
  assert!(fs::metadata("input.txt")?.is_file());
}
```

You can mix both styles in the same include:

```rust
use assay::assay;

#[assay(include = [
  "Cargo.toml",                     // → <temp>/Cargo.toml
  ("src/lib.rs", "sources/lib.rs"), // → <temp>/sources/lib.rs
])]
fn mixed_include() {
  assert!(fs::metadata("Cargo.toml")?.is_file());
  assert!(fs::metadata("sources/lib.rs")?.is_file());
}
```

## Panics
`assay` works with the standard `#[should_panic]` attribute:

```rust
use assay::assay;

#[should_panic]
#[assay]
fn panic_test() {
  panic!("Panic! At The Proc-Macro");
}

#[should_panic(expected = "specific message")]
#[assay]
fn panic_with_message() {
  panic!("specific message");
}
```

## Ignored Tests

Use the standard `#[ignore]` attribute to skip tests:

```rust
use assay::assay;

#[ignore]
#[assay]
fn slow_test() {
  // This test is skipped by default
}
```

## Async Tests

Assay works with any async executor. Simply stack your executor's test attribute with `#[assay]`:

```rust
use assay::assay;

#[tokio::test]
#[assay]
async fn my_async_test() {
    let result = some_async_operation().await?;
    assert_eq!(result, expected);
}
```

Supported executors include:
- `#[tokio::test]`
- `#[actix_rt::test]`
- `#[async_std::test]`
- `#[smol_potat::test]`
- And any other executor that provides a test attribute

For simple futures that don't require a full runtime (no I/O, timers, or spawning), you can use `#[assay]` alone:

```rust
use assay::assay;

#[assay]
async fn simple_async_test() {
    let value = std::future::ready(42).await;
    assert_eq!(value, 42);
}
```

This uses a lightweight built-in executor suitable for basic async operations.

## Setup and Teardown Functions

Sometimes you need to setup the same things all the time and maybe with
different inputs. You might also need to handle tearing down things in the same
way. You can define a function call expression like so with `?` support and
different parameters as input. Just define `setup` or `teardown` in your macro
with the function you want used before or after the test. Note
`before_each`/`after_each` support for `assay` does not exist yet as we'd need
some kind of macro for the file itself to modify the args to `assay`.

```rust
use assay::assay;
use std::{
  env,
  fs,
  path::PathBuf,
};

#[assay(
  setup = setup_func(5)?,
  teardown = teardown_func(),
)]
fn setup_teardown_test() {
  assert_eq!(fs::read_to_string("setup")?, "Value: 5");
}

fn setup_func(input: i32) -> Result<(), Box<dyn std::error::Error>> {
  fs::write("setup", format!("Value: {}", input))?;
  Ok(())
}

fn teardown_func() {
  fs::remove_file("setup").unwrap();
  assert!(!PathBuf::from("setup").exists());
}
```

## Timeout

You can set a timeout for tests to prevent them from hanging indefinitely:

```rust
use assay::assay;

#[assay(timeout = "30s")]  // Test must complete within 30 seconds
fn network_test() {
  // If this takes longer than 30 seconds, the test fails
}

#[assay(timeout = "500ms")]  // Milliseconds for fast tests
fn quick_test() {
  // Must complete within 500 milliseconds
}

#[assay(timeout = "2m")]  // Minutes for slow integration tests
fn slow_integration_test() {
  // Must complete within 2 minutes
}
```

Supported duration formats:
- Seconds: `"30s"`, `"30sec"`, `"30 seconds"`
- Milliseconds: `"500ms"`, `"500 millis"`
- Minutes: `"2m"`, `"2min"`, `"2 minutes"`

The timeout covers the entire test execution including setup and teardown functions.

**Note**: When using `cargo-nextest` with `process-per-test` mode, the timeout
attribute is ignored in favor of nextest's native timeout configuration.
Configure timeouts in `.config/nextest.toml` instead.

## Retries

You can configure tests to retry on failure. This is useful for flaky tests or
tests that depend on external resources that may occasionally be unavailable:

```rust
use assay::assay;

#[assay(retries = 3)]  // Test will run up to 3 times total
fn flaky_network_test() {
  // If this fails, it will be retried up to 2 more times
  // If any attempt passes, the test passes silently
}
```

When combined with timeout, each retry attempt gets its own fresh timeout:

```rust
use assay::assay;

#[assay(retries = 3, timeout = "5s")]
fn retried_with_timeout() {
  // Each of the 3 attempts can take up to 5 seconds
}
```

**Note**: When using `cargo-nextest` with `process-per-test` mode, the retries
attribute is ignored. Configure retries in `.config/nextest.toml` instead using
nextest's native retry configuration.

## Parameterized Tests - Cases

Run the same test logic with different named test cases. Each case generates
a separate test function with a descriptive name:

```rust
use assay::assay;

#[assay(
  cases = [
    positive: (2, 3, 5),
    zeros: (0, 0, 0),
    negative: (-1, -2, -3),
  ]
)]
fn test_addition(a: i32, b: i32, expected: i32) {
  assert_eq!(a + b, expected);
}
```

This generates three tests:
- `test_addition_positive` - tests `2 + 3 = 5`
- `test_addition_zeros` - tests `0 + 0 = 0`
- `test_addition_negative` - tests `-1 + -2 = -3`

Cases work with strings and other types too:

```rust
use assay::assay;

#[assay(
  cases = [
    hello: ("hello", 5),
    empty: ("", 0),
    spaces: ("a b c", 5),
  ]
)]
fn test_string_length(s: &str, expected: usize) {
  assert_eq!(s.len(), expected);
}
```

## Parameterized Tests - Matrix

Generate all combinations of parameter values automatically. This is useful
when you want to test every combination of inputs:

```rust
use assay::assay;

#[assay(
  matrix = [
    a: [1, 2],
    b: [10, 20],
  ]
)]
fn test_multiply(a: i32, b: i32) {
  assert!(a * b >= 10);
}
```

This generates four tests for all combinations:
- `test_multiply_1_10` - tests with `a=1, b=10`
- `test_multiply_1_20` - tests with `a=1, b=20`
- `test_multiply_2_10` - tests with `a=2, b=10`
- `test_multiply_2_20` - tests with `a=2, b=20`

Matrix parameters must match function parameters in order:

```rust
use assay::assay;

#[assay(
  matrix = [
    x: [true, false],
    y: [true, false],
  ]
)]
fn test_booleans(x: bool, y: bool) {
  // Tests all 4 combinations: (true,true), (true,false), (false,true), (false,false)
  let _ = x && y;
}
```

**Note**: `cases` and `matrix` are mutually exclusive - use one or the other.

## Putting it all together!

These features can be combined as they use a comma separated list and so you
could do something like this:

```rust
use assay::assay;
use std::{
  env,
  fs,
  future::Future,
  path::PathBuf,
  pin::Pin,
  task::{Poll, Context},
};

#[tokio::test]
#[should_panic]
#[assay(
  setup = setup_func(5)?,
  env = [
    ("GOODBOY", "Bukka"),
    ("BADDOGS", "false")
  ],
  teardown = teardown_func(),
  include = ["Cargo.toml", "src/lib.rs"],
)]
async fn one_test_to_call_it_all() {
  ReadyOnPoll.await;

  assert_eq!(env::var("GOODBOY")?, "Bukka");
  assert_eq!(env::var("BADDOGS")?, "false");
  assert_eq!(fs::read_to_string("setup")?, "Value: 5");
  // Files are at temp root with filename only
  assert!(PathBuf::from("Cargo.toml").exists());
  assert!(PathBuf::from("lib.rs").exists());

  // Removing this actually causes the test to fail
  panic!();
}

struct ReadyOnPoll;
impl Future for ReadyOnPoll {
  type Output = ();
  fn poll(self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
    Poll::Ready(())
  }
}

fn setup_func(input: i32) -> Result<(), Box<dyn std::error::Error>> {
  fs::write("setup", format!("Value: {}", input))?;
  Ok(())
}

fn teardown_func() {
  fs::remove_file("setup").unwrap();
  assert!(!PathBuf::from("setup").exists());
}
```

Use as many or as few features as you need!

# Limitations
While `assay` is capable of a lot right now it's not without issues:
- `assay` does not work inside doc tests!

# MSRV Policy
We do not have a Minimum Supported Rust Version and only track `stable`. Older
versions might work, but it's not guaranteed.

# License
All files within this project are distributed under the Mozilla Public License
version 2.0. You can read the terms of the license [here](https://www.mozilla.org/en-US/MPL/2.0/).
