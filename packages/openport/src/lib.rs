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

/// A port reservation system with an exclusive range (e.g., `15000..16000`)
///
/// This is a type alias for a port reservation system with a [`Range<Port>`] (exclusive range).
/// For inclusive ranges (e.g., `15000..=16000`), construct a `PortReservation<RangeInclusive<Port>>` directly.
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "reservation")]
/// # {
/// use openport::PortReservation;
///
/// let reservation = PortReservation::new(15000..16000);
/// let port = reservation.reserve_port().expect("No ports available");
/// # }
/// ```
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
    /// Converts the range into an iterator of port numbers by consuming the range
    ///
    /// # Returns
    ///
    /// Returns an iterator that yields each port number in the range
    fn into_iter(self) -> impl Iterator<Item = u16>;

    /// Creates an iterator of port numbers from a borrowed range
    ///
    /// # Returns
    ///
    /// Returns an iterator that yields each port number in the range
    fn iter(&self) -> impl Iterator<Item = u16>;
}

/// Implementation of [`PortRange`] for exclusive ranges (`start..end`)
///
/// Allows using exclusive port ranges with [`pick_unused_port`].
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

/// Implementation of [`PortRange`] for inclusive ranges (`start..=end`)
///
/// Allows using inclusive port ranges with [`pick_unused_port`].
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
    use super::{PortRange, is_free, is_free_tcp, is_free_udp, pick_unused_port};
    use std::net::{TcpListener, UdpSocket};

    #[cfg(feature = "rand")]
    use super::pick_random_unused_port;

    #[cfg(feature = "rand")]
    #[test_log::test]
    fn it_works() {
        assert!(pick_random_unused_port().is_some());
    }

    #[test_log::test]
    fn port_range_test() {
        if let Some(p) = pick_unused_port(15000..16000) {
            assert!((15000..16000).contains(&p));
        }
        if let Some(p) = pick_unused_port(20000..21000) {
            assert!((20000..21000).contains(&p));
        }
    }

    #[test_log::test]
    fn port_range_inclusize_test() {
        if let Some(p) = pick_unused_port(15000..=16000) {
            assert!((15000..=16000).contains(&p));
        }
        if let Some(p) = pick_unused_port(20000..=21000) {
            assert!((20000..=21000).contains(&p));
        }
    }

    #[test_log::test]
    fn test_is_free_tcp() {
        // Try multiple times to find a port and bind to it
        for _ in 0..10 {
            if let Some(port) = pick_unused_port(15000..16000) {
                // Port should be free initially
                if is_free_tcp(port) {
                    // Bind to the port
                    if let Ok(_listener) = TcpListener::bind(("0.0.0.0", port)) {
                        // Port should now be occupied
                        assert!(!is_free_tcp(port));
                        return; // Test passed
                    }
                }
            }
        }
        panic!("Could not find a port to test with after 10 attempts");
    }

    #[test_log::test]
    fn test_is_free_udp() {
        // Try multiple times to find a port and bind to it
        for _ in 0..10 {
            if let Some(port) = pick_unused_port(15000..16000) {
                // Port should be free initially
                if is_free_udp(port) {
                    // Bind to the port
                    if let Ok(_socket) = UdpSocket::bind(("0.0.0.0", port)) {
                        // Port should now be occupied
                        assert!(!is_free_udp(port));
                        return; // Test passed
                    }
                }
            }
        }
        panic!("Could not find a port to test with after 10 attempts");
    }

    #[test_log::test]
    fn test_is_free() {
        // Try multiple times to find a port and bind to it
        for _ in 0..10 {
            if let Some(port) = pick_unused_port(15000..16000) {
                // Port should be free on both TCP and UDP initially
                if is_free(port) {
                    // Bind to the port with TCP
                    if let Ok(_listener) = TcpListener::bind(("0.0.0.0", port)) {
                        // Wait for the port to be occupied
                        for _ in 0..10 {
                            if !is_free(port) {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        assert!(!is_free(port));
                        return; // Test passed
                    }
                }
            }
        }
        panic!("Could not find a port to test with after 10 attempts");
    }

    #[test_log::test]
    fn test_is_free_udp_binding() {
        // Try multiple times to find a port and bind to it
        for _ in 0..10 {
            if let Some(port) = pick_unused_port(15000..16000) {
                // Port should be free on both TCP and UDP initially
                if is_free(port) {
                    // Bind to the port with UDP
                    if let Ok(_socket) = UdpSocket::bind(("0.0.0.0", port)) {
                        // Port should now be occupied
                        assert!(!is_free(port));
                        return; // Test passed
                    }
                }
            }
        }
        panic!("Could not find a port to test with after 10 attempts");
    }

    #[test_log::test]
    fn test_port_range_trait_exclusive() {
        let range = 15000..15010;

        // Test iter method
        let ports: Vec<u16> = range.iter().collect();
        assert_eq!(ports.len(), 10);
        assert_eq!(ports[0], 15000);
        assert_eq!(ports[9], 15009);

        // Test into_iter method
        let range2 = 15000..15010;
        let ports2: Vec<u16> = PortRange::into_iter(range2).collect();
        assert_eq!(ports2.len(), 10);
        assert_eq!(ports2[0], 15000);
        assert_eq!(ports2[9], 15009);
    }

    #[test_log::test]
    fn test_port_range_trait_inclusive() {
        let range = 15000..=15010;

        // Test iter method
        let ports: Vec<u16> = range.iter().collect();
        assert_eq!(ports.len(), 11);
        assert_eq!(ports[0], 15000);
        assert_eq!(ports[10], 15010);

        // Test into_iter method
        let range2 = 15000..=15010;
        let ports2: Vec<u16> = PortRange::into_iter(range2).collect();
        assert_eq!(ports2.len(), 11);
        assert_eq!(ports2[0], 15000);
        assert_eq!(ports2[10], 15010);
    }

    #[test_log::test]
    fn test_port_range_empty() {
        // Test with empty exclusive range
        let range = 15000..15000;
        assert_eq!(range.iter().count(), 0);

        // Test with empty range doesn't find any ports
        let result = pick_unused_port(15000..15000);
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_port_range_single_port() {
        // Test with inclusive range containing a single port
        let range = 15000..=15000;
        let ports: Vec<u16> = range.iter().collect();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0], 15000);
    }
}
