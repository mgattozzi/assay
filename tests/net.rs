/*
 * Copyright (C) 2021 - 2025 Michael Gattozzi <michael@ductile.systems>
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use assay::assay;
use std::net::IpAddr;
use std::net::TcpListener;
use std::net::UdpSocket;

#[assay]
/// This checks that we are including the `assay::net::TestAddress` trait and that this works for TcpListener
fn tcp_addr() {
  let ipv4 = TcpListener::test_v4()?;
  let ipv6 = TcpListener::test_v6()?;

  let ipv4_addr = ipv4.local_addr()?;
  assert!(ipv4_addr.is_ipv4());
  assert!(ipv4_addr.port() > 0);

  let ipv6_addr = ipv6.local_addr()?;
  assert!(ipv6_addr.is_ipv6());
  assert!(ipv6_addr.port() > 0);
}

#[assay]
/// This checks that we are including the `assay::net::TestAddress` trait and that this works for UpdSocket
fn udp_addr() {
  let ipv4 = UdpSocket::test_v4()?;
  let ipv6 = UdpSocket::test_v6()?;

  let ipv4_addr = ipv4.local_addr()?;
  assert!(ipv4_addr.is_ipv4());
  assert!(ipv4_addr.port() > 0);

  let ipv6_addr = ipv6.local_addr()?;
  assert!(ipv6_addr.is_ipv6());
  assert_eq!(
    ipv6_addr.ip(),
    IpAddr::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
  );
  assert!(ipv6_addr.port() > 0);
}
