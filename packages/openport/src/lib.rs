#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
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

#[cfg(feature = "reservation")]
mod reservation;

/// A network port number
pub type Port = u16;

#[cfg(feature = "reservation")]
pub type PortReservation = reservation::PortReservation<Range<Port>>;

// Try to bind to a socket using UDP
fn test_bind_udp<A: ToSocketAddrs>(addr: A) -> Option<Port> {
    Some(UdpSocket::bind(addr).ok()?.local_addr().ok()?.port())
}

// Try to bind to a socket using TCP
fn test_bind_tcp<A: ToSocketAddrs>(addr: A) -> Option<Port> {
    Some(TcpListener::bind(addr).ok()?.local_addr().ok()?.port())
}

/// Check if a port is free on UDP
///
/// # Parameters
///
/// * `port` - The port number to check for availability
///
/// # Returns
///
/// Returns `true` if the port is free on both IPv4 and IPv6, `false` otherwise
#[must_use]
pub fn is_free_udp(port: Port) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0);

    test_bind_udp(ipv6).is_some() && test_bind_udp(ipv4).is_some()
}

/// Check if a port is free on TCP
///
/// # Parameters
///
/// * `port` - The port number to check for availability
///
/// # Returns
///
/// Returns `true` if the port is free on both IPv4 and IPv6, `false` otherwise
#[must_use]
pub fn is_free_tcp(port: Port) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0);

    test_bind_tcp(ipv6).is_some() && test_bind_tcp(ipv4).is_some()
}

/// Check if a port is free on both TCP and UDP
///
/// # Parameters
///
/// * `port` - The port number to check for availability
///
/// # Returns
///
/// Returns `true` if the port is free on both TCP and UDP (for both IPv4 and IPv6), `false` otherwise
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
///
/// This function first tries to find a random port in the range `15000..25000`,
/// then falls back to asking the OS for a free port if no random port is available.
///
/// # Returns
///
/// Returns `Some(port)` if a free port is found, or `None` if no ports are available
/// after 20 attempts (10 random attempts + 10 OS-provided attempts)
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "rand")]
/// # {
/// use openport::pick_random_unused_port;
/// let port: u16 = pick_random_unused_port().expect("No ports free");
/// # }
/// ```
#[cfg(feature = "rand")]
#[must_use]
pub fn pick_random_unused_port() -> Option<Port> {
    // Try random port first
    for _ in 0..10 {
        let port = switchy_random::rng().gen_range(15000..25000);
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

/// Trait for types that can be converted into an iterator of port numbers
///
/// This trait is implemented for [`Range<u16>`] and [`RangeInclusive<u16>`], allowing
/// [`pick_unused_port`] to accept both exclusive and inclusive port ranges.
pub trait PortRange {
    /// Converts the range into an iterator of port numbers
    fn into_iter(self) -> impl Iterator<Item = u16>;

    /// Converts the range into an iterator of port numbers
    fn iter(&self) -> impl Iterator<Item = u16>;
}

impl PortRange for Range<u16> {
    #[inline]
    fn into_iter(self) -> impl Iterator<Item = u16> {
        <Self as IntoIterator>::into_iter(self)
    }

    #[inline]
    fn iter(&self) -> impl Iterator<Item = u16> {
        self.clone()
    }
}

impl PortRange for RangeInclusive<u16> {
    #[inline]
    fn into_iter(self) -> impl Iterator<Item = u16> {
        <Self as IntoIterator>::into_iter(self)
    }

    #[inline]
    fn iter(&self) -> impl Iterator<Item = u16> {
        self.clone()
    }
}

/// Picks an available port that is available on both TCP and UDP within a range
///
/// # Parameters
///
/// * `range` - A port range to search within (can be exclusive `start..end` or inclusive `start..=end`)
///
/// # Returns
///
/// Returns `Some(port)` with the first free port found in the range, or `None` if no free ports
/// are available in the specified range
///
/// # Examples
///
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
