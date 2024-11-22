use assay::assay;
use std::process::Command;

#[assay(ignore)]
fn assert_eq() {
  assert_eq!(1, 5);
}

#[assay(ignore)]
fn assert_ne() {
  assert_ne!(["foo", "bar"], ["foo", "bar"]);
}

#[assay(ignore)]
fn assert_eq_sorted() {
  assert_eq_sorted!([1, 3, 2], [1, 2, 4]);
}

#[test]
fn pretty_assertions() {
  let output = Command::new("cargo")
    .args(["test", "--workspace", "--", "--ignored", "assert"])
    .output()
    .unwrap();
  let assert_tests = String::from_utf8(output.stdout).unwrap();

  if assert_tests.contains(
    "---- assert_eq_sorted stdout ----
thread 'assert_eq_sorted' panicked at tests/pretty_assert.rs:16:3:
assertion failed: `(left == right)`

Diff < left / right > :
 [
     1,
<    3,
     2,
>    4,
 ]",
  ) && assert_tests.contains(
    "---- assert_eq stdout ----
thread 'assert_eq' panicked at tests/pretty_assert.rs:6:3:
assertion failed: `(left == right)`

Diff < left / right > :
<1
>5",
  ) && assert_tests.contains(
    "
---- assert_ne stdout ----
thread 'assert_ne' panicked at tests/pretty_assert.rs:11:3:
assertion failed: `(left != right)`

Both sides:
[
    \"foo\",
    \"bar\",
]",
  ) && assert_tests.contains(
    "failures:
    assert_eq
    assert_eq_sorted
    assert_ne

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 1 filtered out",
  ) {
    panic!(
      "Unexpected output for assertions.\n\nOutput:\n{}",
      assert_tests
    );
  }
}
