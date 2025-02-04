/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Traits and types made to make testing with network services/code much easer

use std::io;
use std::net::{TcpListener, UdpSocket};

/// Generate a bound address with either ipv4 or ipv6 that won't conflict with other addresses
pub trait TestAddress
where
  Self: Sized,
{
  /// Obtain a bound ipv4 address for implementors of this trait
  fn test_v4() -> Result<Self, io::Error>;
  /// Obtain a bound ipv6 address for implementors of this trait
  fn test_v6() -> Result<Self, io::Error>;
}

impl TestAddress for TcpListener {
  fn test_v4() -> Result<Self, io::Error> {
    Self::bind(("0.0.0.0", 0))
  }
  fn test_v6() -> Result<Self, io::Error> {
    Self::bind(("::", 0))
  }
}

impl TestAddress for UdpSocket {
  fn test_v4() -> Result<Self, io::Error> {
    Self::bind(("0.0.0.0", 0))
  }
  fn test_v6() -> Result<Self, io::Error> {
    Self::bind(("::", 0))
  }
}
