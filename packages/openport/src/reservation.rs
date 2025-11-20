//! Port reservation system for managing port allocation
//!
//! This module provides a thread-safe port reservation system that allows you to
//! reserve and release ports from a specified range. This is useful for applications
//! that need to dynamically allocate multiple network services on different ports
//! while ensuring no conflicts occur.
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(feature = "reservation")]
//! # {
//! use openport::PortReservation;
//!
//! let reservation = PortReservation::new(15000..16000);
//! let port = reservation.reserve_port().expect("No ports available");
//! assert!(reservation.is_reserved(port));
//! reservation.release_port(port);
//! assert!(!reservation.is_reserved(port));
//! # }
//! ```

use std::collections::BTreeSet;
use std::sync::Mutex;

use crate::{Port, PortRange, is_free};

/// A port reservation system that manages port allocation within a specified range
///
/// `PortReservation` allows you to reserve and release ports from a given range,
/// ensuring that the same port is not allocated multiple times. This is useful
/// for applications that need to manage multiple network services on different ports.
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
/// assert!(reservation.is_reserved(port));
/// reservation.release_port(port);
/// assert!(!reservation.is_reserved(port));
/// # }
/// ```
pub struct PortReservation<R: PortRange> {
    /// The range of ports that can be reserved
    range: R,
    /// Set of currently reserved ports, protected by a mutex for thread-safe access
    reserved_ports: Mutex<BTreeSet<Port>>,
}

/// Default implementation for [`PortReservation`] with exclusive range
///
/// Creates a port reservation system with the default range of `15000..65535`.
/// This provides a large pool of ports in the dynamic/private port range.
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "reservation")]
/// # {
/// use openport::PortReservation;
///
/// let reservation: PortReservation = PortReservation::default();
/// let port = reservation.reserve_port().expect("No ports available");
/// assert!((15000..65535).contains(&port));
/// # }
/// ```
impl Default for PortReservation<std::ops::Range<Port>> {
    fn default() -> Self {
        Self::new(15000..65535)
    }
}

/// Default implementation for [`PortReservation`] with inclusive range
///
/// Creates a port reservation system with the default range of `15000..=65535`.
/// This provides a large pool of ports in the dynamic/private port range.
///
/// Note: Since the `reservation` module is private, this implementation is primarily
/// used internally. Users should use the exported `PortReservation` type alias for
/// exclusive ranges, or construct instances with `new()` for inclusive ranges.
impl Default for PortReservation<std::ops::RangeInclusive<Port>> {
    fn default() -> Self {
        Self::new(15000..=65535)
    }
}

impl<R: PortRange> PortReservation<R> {
    /// Creates a new port reservation system with the specified range
    ///
    /// # Parameters
    ///
    /// * `range` - The range of ports that can be reserved (can be exclusive `start..end` or inclusive `start..=end`)
    ///
    /// # Returns
    ///
    /// Returns a new `PortReservation` instance with no ports initially reserved
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "reservation")]
    /// # {
    /// use openport::PortReservation;
    ///
    /// let reservation = PortReservation::new(15000..16000);
    /// # }
    /// ```
    #[must_use]
    pub const fn new(range: R) -> Self {
        Self {
            range,
            reserved_ports: Mutex::new(BTreeSet::new()),
        }
    }

    fn is_free(ports: &BTreeSet<Port>, port: Port) -> bool {
        !ports.contains(&port) && is_free(port)
    }

    /// Reserve a port for use
    ///
    /// # Parameters
    ///
    /// * `num_ports` - The number of ports to reserve
    ///
    /// # Panics
    ///
    /// * If `reserved_ports` lock is poisoned
    ///
    /// # Returns
    ///
    /// Returns a vector of reserved ports
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "reservation")]
    /// # {
    /// use openport::PortReservation;
    /// let reservation = PortReservation::new(15000..16000);
    /// let ports = reservation.reserve_ports(10);
    /// for port in ports {
    ///     assert!(reservation.is_reserved(port));
    /// }
    /// # }
    /// ```
    pub fn reserve_ports(&self, num_ports: usize) -> Vec<Port> {
        let mut reserved_ports = self.reserved_ports.lock().unwrap();
        let mut ports = Vec::new();

        for port in self.range.iter() {
            if ports.len() >= num_ports {
                break;
            }

            // Use the existing is_free function to check if the port is free
            if Self::is_free(&reserved_ports, port) {
                reserved_ports.insert(port);
                ports.push(port);
            }
        }
        drop(reserved_ports);

        ports
    }

    /// Reserve a port for use
    ///
    /// # Panics
    ///
    /// * If `reserved_ports` lock is poisoned
    ///
    /// # Returns
    ///
    /// Returns the first reserved port, or `None` if no ports are available
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "reservation")]
    /// # {
    /// use openport::PortReservation;
    /// let reservation = PortReservation::new(15000..16000);
    /// let port = reservation.reserve_port();
    /// assert!(reservation.is_reserved(port.unwrap()));
    /// # }
    /// ```
    pub fn reserve_port(&self) -> Option<Port> {
        let mut reserved_ports = self.reserved_ports.lock().unwrap();

        let port = self
            .range
            .iter()
            .find(|x| Self::is_free(&reserved_ports, *x))?;

        reserved_ports.insert(port);

        drop(reserved_ports);

        Some(port)
    }

    /// Release reserved ports
    ///
    /// # Parameters
    ///
    /// * `ports` - The ports to release
    ///
    /// # Panics
    ///
    /// * If `reserved_ports` lock is poisoned
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "reservation")]
    /// # {
    /// use openport::PortReservation;
    /// let reservation = PortReservation::new(15000..16000);
    /// let ports = reservation.reserve_ports(10);
    /// reservation.release_ports(ports.iter().copied());
    /// for port in ports {
    ///     assert!(!reservation.is_reserved(port));
    /// }
    /// # }
    /// ```
    pub fn release_ports(&self, ports: impl Iterator<Item = Port>) {
        let mut reserved_ports = self.reserved_ports.lock().unwrap();
        for port in ports {
            reserved_ports.remove(&port);
        }
    }

    /// Release a reserved port
    ///
    /// # Parameters
    ///
    /// * `port` - The port to release
    ///
    /// # Panics
    ///
    /// * If `reserved_ports` lock is poisoned
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "reservation")]
    /// # {
    /// use openport::PortReservation;
    /// let reservation = PortReservation::new(15000..16000);
    /// let ports = reservation.reserve_ports(10);
    /// reservation.release_port(ports[0]);
    /// assert!(!reservation.is_reserved(ports[0]));
    /// # }
    /// ```
    pub fn release_port(&self, port: Port) {
        self.reserved_ports.lock().unwrap().remove(&port);
    }

    /// Check if a port is reserved
    ///
    /// # Parameters
    ///
    /// * `port` - The port to check
    ///
    /// # Panics
    ///
    /// * If `reserved_ports` lock is poisoned
    ///
    /// # Returns
    ///
    /// Returns `true` if the port is reserved, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "reservation")]
    /// # {
    /// use openport::PortReservation;
    /// let reservation = PortReservation::new(15000..16000);
    /// let ports = reservation.reserve_ports(10);
    /// assert!(reservation.is_reserved(ports[0]));
    /// # }
    /// ```
    #[must_use]
    pub fn is_reserved(&self, port: Port) -> bool {
        self.reserved_ports.lock().unwrap().contains(&port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reserve_port() {
        let reservation = PortReservation::new(15000..15100);
        let port = reservation.reserve_port();

        assert!(port.is_some());
        let port = port.unwrap();
        assert!((15000..15100).contains(&port));
        assert!(reservation.is_reserved(port));
    }

    #[test]
    fn test_reserve_ports() {
        let reservation = PortReservation::new(15000..15100);
        let ports = reservation.reserve_ports(5);

        assert_eq!(ports.len(), 5);
        for port in &ports {
            assert!((15000..15100).contains(port));
            assert!(reservation.is_reserved(*port));
        }

        // Verify no duplicates
        let mut unique_ports = ports.clone();
        unique_ports.sort_unstable();
        unique_ports.dedup();
        assert_eq!(unique_ports.len(), ports.len());
    }

    #[test]
    fn test_release_port() {
        let reservation = PortReservation::new(15000..15100);
        let port = reservation.reserve_port().unwrap();

        assert!(reservation.is_reserved(port));
        reservation.release_port(port);
        assert!(!reservation.is_reserved(port));
    }

    #[test]
    fn test_release_ports() {
        let reservation = PortReservation::new(15000..15100);
        let ports = reservation.reserve_ports(10);

        assert_eq!(ports.len(), 10);
        for port in &ports {
            assert!(reservation.is_reserved(*port));
        }

        reservation.release_ports(ports.iter().copied());

        for port in ports {
            assert!(!reservation.is_reserved(port));
        }
    }

    #[test]
    fn test_default_implementation() {
        let reservation: PortReservation<std::ops::Range<u16>> = PortReservation::default();
        let port = reservation.reserve_port();
        assert!(port.is_some());
        assert!((15000..65535).contains(&port.unwrap()));
    }

    #[test]
    fn test_default_implementation_inclusive() {
        let reservation: PortReservation<std::ops::RangeInclusive<u16>> =
            PortReservation::default();
        let port = reservation.reserve_port();
        assert!(port.is_some());
        assert!((15000..=65535).contains(&port.unwrap()));
    }

    #[test]
    fn test_reserve_more_than_available() {
        // Use a very small range to test this edge case
        let reservation = PortReservation::new(15000..15002);
        let ports = reservation.reserve_ports(10); // Try to reserve 10 from a range of 2

        // Should get at most 2 ports (the number available in the range)
        assert!(ports.len() <= 2);
        assert!(!ports.is_empty()); // Should still get some ports if available

        for port in ports {
            assert!((15000..15002).contains(&port));
            assert!(reservation.is_reserved(port));
        }
    }

    #[test]
    fn test_no_free_ports() {
        // Use a range that's likely to have no free ports (very high numbers)
        // Note: This test might be flaky on different systems, but it's worth testing
        let reservation = PortReservation::new(65530..65535);
        let ports = reservation.reserve_ports(5);

        // We might get some ports or none, depending on system state
        // The important thing is that the function doesn't panic
        for port in ports {
            assert!((65530..65535).contains(&port));
        }
    }

    #[test]
    fn test_inclusive_range() {
        let reservation = PortReservation::new(15000..=15010);
        let ports = reservation.reserve_ports(5);

        assert_eq!(ports.len(), 5);
        for port in ports {
            assert!((15000..=15010).contains(&port));
            assert!(reservation.is_reserved(port));
        }
    }

    #[test]
    fn test_reserve_after_release() {
        let reservation = PortReservation::new(15000..15100);

        // Reserve some ports
        let ports = reservation.reserve_ports(3);
        assert_eq!(ports.len(), 3);

        // Release them
        reservation.release_ports(ports.iter().copied());

        // Reserve again - should be able to get the same ports
        let new_ports = reservation.reserve_ports(3);
        assert_eq!(new_ports.len(), 3);

        // The new ports might be the same or different, but all should be reserved
        for port in new_ports {
            assert!((15000..15100).contains(&port));
            assert!(reservation.is_reserved(port));
        }
    }
}
