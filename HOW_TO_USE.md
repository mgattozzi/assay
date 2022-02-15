## How to use `assay`

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

### Basic Usage & Automatic Niceties

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

### Env Vars
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

### Include files
Sometimes you want to include files in your tests and generating them is one
way, but having it in your version control system and then having them be in
your tests can also be nice! With the `include` directive you can include files
in your test's directory when you start running it:

```rust
use assay::assay;

#[assay(include = ["Cargo.toml", "src/lib.rs"])]
fn include() {
  assert!(fs::metadata("src/lib.rs")?.is_file());
  assert!(fs::metadata("Cargo.toml")?.is_file());
}
```

### Panics
`assay` will also let you mark a test that you expect to panic much like you
would for a normal Rust test:

```rust
use assay::assay;

#[assay(should_panic)]
fn panic_test() {
  panic!("Panic! At The Proc-Macro");
}
```

### `async` tests
If you want your tests to run `async` code all you need to do is specify that the
test is `async`. Note this won't let you control the runtime currently. `assay` only
uses the default `tokio` executor.

```rust
use assay::assay;
use std::{
  pin::Pin,
  future::Future,
  task::{Poll, Context},
};

#[assay]
async fn async_func() {
  ReadyOnPoll.await;
}

struct ReadyOnPoll;
impl Future for ReadyOnPoll {
  type Output = ();
  fn poll(self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
    Poll::Ready(())
  }
}
```

### Setup and Teardown Functions

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
### Putting it all together!

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
async fn one_test_to_call_it_all() {
  ReadyOnPoll.await;

  assert_eq!(env::var("GOODBOY")?, "Bukka");
  assert_eq!(env::var("BADDOGS")?, "false");
  assert_eq!(fs::read_to_string("setup")?, "Value: 5");
  assert!(PathBuf::from("Cargo.toml").exists());
  assert!(PathBuf::from("src/lib.rs").exists());

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

## Limitations
While `assay` is capable of a lot right now it's not without issues:

- Tests run in their own process and so getting the output available in a good
  way is still kind of an open problem
- Sometimes tests that shouldn't pass do, at least when having developed assay,
  because they run in another process. You should intentionally crash your test
  to make sure it's actually working, because you'll have tests pass that really
  shouldn't which frankly isn't great
- Rust Analyzer gets tripped up sometimes and the error propagates to each
  invocation making it harder to track down. In these cases `cargo test` will
  let you know where the issue actually is
- No work on spans yet! This macro just slaps things in and so error messages
  are much to be desired without much in the way to tell you why an invocation
  of `assay` fails.
- `assay` does not work inside doc tests!
