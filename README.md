# assay - A super powered testing macro for Rust

> as·say /ˈaˌsā,aˈsā/ noun - the testing of a metal or ore to determine its ingredients and quality.

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
- Automatically importing the `pretty_assertions` crate so that you can have
  pretty output for `assert_eq` and `assert_ne`
- Allowing you to define setup and teardown functions to call for the test

`assay` was born out of personal frustration with the way things are and wanting
to handle the boilerplate without needing to write a whole test framework, while
also pushing the bounds of what we could have today on stable Rust.

# Limitations
While `assay` is capable of a lot right now it's not without issues:

- Tests run in their own process and so getting the output available in a good
  way is still kind of an open problem
- Sometimes tests that shouldn't pass do, at least when having developed `assay`,
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

# How to use `assay`
Take a look at [`HOW_TO_USE.md`](HOW_TO_USE.md) (which is included in the crate
documentation) or [`tests/integration_tests.rs`](tests/integration_tests.rs).

# MSRV Policy
We do not have a Minimum Supported Rust Version and only track `stable`. Given
this crate uses 2021 edition `rustc` >= 1.56 for now, but that's not always
guaranteed to be the case and later versions might require a greater version
than 1.56.

# License
All files within this project are distributed under the Mozilla Public License
version 2.0. You can read the terms of the license in [`LICENSE.txt`](LICENSE.txt).
