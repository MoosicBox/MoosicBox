#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![forbid(unsafe_code)]

//! # openport
//!
//! Find a free unused port
//!
//! # Features
//!
//! *   `rand`: Add `pick_random_unused_port` pub fn that allows finding a random port within
//!     the range `15000..25000`
//!
//! # Usage
//!
//! The following steps describe a basic usage of openport:
//!
//! 1.  Call `openport::pick_unused_port` and pass a range of ports you want to find a free port in
//! 2.  Enable the `rand` feature and call `openport::pick_random_unused_port` to find a random open
//!     port within the range `15000..16000`
//! 3.  Call `openport::is_free` to check if a specific port is open on both TCP and UDP
//! 4.  Call `openport::is_free_tcp` to check if a specific port is open on TCP
//! 5.  Call `openport::is_free_udp` to check if a specific port is open on UDP

use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6, TcpListener, ToSocketAddrs, UdpSocket},
    ops::{Range, RangeInclusive},
};

pub type Port = u16;

// Try to bind to a socket using UDP
fn test_bind_udp<A: ToSocketAddrs>(addr: A) -> Option<Port> {
    Some(UdpSocket::bind(addr).ok()?.local_addr().ok()?.port())
}

// Try to bind to a socket using TCP
fn test_bind_tcp<A: ToSocketAddrs>(addr: A) -> Option<Port> {
    Some(TcpListener::bind(addr).ok()?.local_addr().ok()?.port())
}

/// Check if a port is free on UDP
#[must_use]
pub fn is_free_udp(port: Port) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0);

    test_bind_udp(ipv6).is_some() && test_bind_udp(ipv4).is_some()
}

/// Check if a port is free on TCP
#[must_use]
pub fn is_free_tcp(port: Port) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0);

    test_bind_tcp(ipv6).is_some() && test_bind_tcp(ipv4).is_some()
}

/// Check if a port is free on both TCP and UDP
#[must_use]
pub fn is_free(port: Port) -> bool {
    is_free_tcp(port) && is_free_udp(port)
}

/// Asks the OS for a free port
#[cfg(feature = "rand")]
fn ask_free_tcp_port() -> Option<Port> {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0);

    test_bind_tcp(ipv6).or_else(|| test_bind_tcp(ipv4))
}

/// Picks an available port that is available on both TCP and UDP
/// ```rust
/// use openport::pick_random_unused_port;
/// let port: u16 = pick_random_unused_port().expect("No ports free");
/// ```
#[cfg(feature = "rand")]
#[must_use]
pub fn pick_random_unused_port() -> Option<Port> {
    use rand::prelude::*;

    let mut rng = rand::rng();

    // Try random port first
    for _ in 0..10 {
        let port = rng.random_range(15000..25000);
        if is_free(port) {
            return Some(port);
        }
    }

    // Ask the OS for a port
    for _ in 0..10 {
        if let Some(port) = ask_free_tcp_port() {
            // Test that the udp port is free as well
            if is_free_udp(port) {
                return Some(port);
            }
        }
    }

    // Give up
    None
}

pub trait PortRange {
    fn into_iter(self) -> impl Iterator<Item = u16>;
}

impl PortRange for Range<u16> {
    #[inline]
    fn into_iter(self) -> impl Iterator<Item = u16> {
        <Self as IntoIterator>::into_iter(self)
    }
}

impl PortRange for RangeInclusive<u16> {
    #[inline]
    fn into_iter(self) -> impl Iterator<Item = u16> {
        <Self as IntoIterator>::into_iter(self)
    }
}

/// Picks an available port that is available on both TCP and UDP within a range
/// ```rust
/// use openport::pick_unused_port;
/// let port: u16 = pick_unused_port(15000..16000).expect("No ports free");
/// ```
#[must_use]
pub fn pick_unused_port(range: impl PortRange) -> Option<Port> {
    range.into_iter().find(|x| is_free(*x))
}

#[cfg(test)]
mod tests {
    use super::pick_unused_port;

    #[cfg(feature = "rand")]
    use super::pick_random_unused_port;

    #[cfg(feature = "rand")]
    #[test]
    fn it_works() {
        assert!(pick_random_unused_port().is_some());
    }

    #[test]
    fn port_range_test() {
        if let Some(p) = pick_unused_port(15000..16000) {
            assert!((15000..16000).contains(&p));
        }
        if let Some(p) = pick_unused_port(20000..21000) {
            assert!((20000..21000).contains(&p));
        }
    }

    #[test]
    fn port_range_inclusize_test() {
        if let Some(p) = pick_unused_port(15000..=16000) {
            assert!((15000..=16000).contains(&p));
        }
        if let Some(p) = pick_unused_port(20000..=21000) {
            assert!((20000..=21000).contains(&p));
        }
    }
}
