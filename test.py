#!/usr/bin/env -S uv run

# /// script
# requires-python = ">=3.12"
# dependencies = ["termcolor == 2.5"]
# ///

from subprocess import run, DEVNULL
from sys import exit
from termcolor import colored

passing_tests = run(["cargo", "test", "--workspace"])
failing_tests = run(
    ["cargo", "test", "--workspace", "--", "--ignored"], stdout=DEVNULL, stderr=DEVNULL
)

passing_tests.check_returncode()
if failing_tests.returncode == 0:
    print(
        colored("ERROR: ", "red")
        + "Ignored tests failed to fail properly. Forking of assay processes is broken somehow"
    )
    print(
        colored("HINT: ", "cyan")
        + "run the tests with 'cargo test --workspace -- --ignored' to see what failed"
    )
    exit(1)
