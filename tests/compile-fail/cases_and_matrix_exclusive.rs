/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use assay::assay;

#[assay(
  cases = [
    one: (1, 2),
  ],
  matrix = [
    a: [1, 2],
    b: [3, 4],
  ],
)]
fn cannot_use_both(a: i32, b: i32) {
  assert!(a + b > 0);
}

fn main() {}
