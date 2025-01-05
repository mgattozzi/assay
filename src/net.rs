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
