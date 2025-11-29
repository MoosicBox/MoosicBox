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

fn reservation_is_free(ports: &BTreeSet<Port>, port: Port) -> bool {
    !ports.contains(&port) && is_free(port)
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
            if reservation_is_free(&reserved_ports, port) {
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
            .find(|x| reservation_is_free(&reserved_ports, *x))?;

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

    /// Check if a port is free
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
    /// Returns `true` if the port is free, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "reservation")]
    /// # {
    /// use openport::PortReservation;
    /// let reservation = PortReservation::new(15000..16000);
    /// let port = reservation.reserve_port();
    /// assert!(reservation.is_free(port.unwrap()));
    /// # }
    /// ```
    #[must_use]
    pub fn is_free(&self, port: Port) -> bool {
        !self.reserved_ports.lock().unwrap().contains(&port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    #[serial]
    fn test_reserve_port() {
        let reservation = PortReservation::new(15000..15100);
        let port = reservation.reserve_port();

        assert!(port.is_some());
        let port = port.unwrap();
        assert!((15000..15100).contains(&port));
        assert!(reservation.is_reserved(port));
    }

    #[test_log::test]
    #[serial]
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

    #[test_log::test]
    #[serial]
    fn test_release_port() {
        let reservation = PortReservation::new(15000..15100);
        let port = reservation.reserve_port().unwrap();

        assert!(reservation.is_reserved(port));
        reservation.release_port(port);
        assert!(!reservation.is_reserved(port));
    }

    #[test_log::test]
    #[serial]
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

    #[test_log::test]
    #[serial]
    fn test_default_implementation() {
        let reservation: PortReservation<std::ops::Range<u16>> = PortReservation::default();
        let port = reservation.reserve_port();
        assert!(port.is_some());
        assert!((15000..65535).contains(&port.unwrap()));
    }

    #[test_log::test]
    #[serial]
    fn test_default_implementation_inclusive() {
        let reservation: PortReservation<std::ops::RangeInclusive<u16>> =
            PortReservation::default();
        let port = reservation.reserve_port();
        assert!(port.is_some());
        assert!((15000..=65535).contains(&port.unwrap()));
    }

    #[test_log::test]
    #[serial]
    fn test_reserve_more_than_available() {
        // Use a very small range to test this edge case
        let mut i = 15000;
        let reservation = loop {
            let reservation = PortReservation::new(i..i + 2);
            if reservation.is_free(i) {
                break reservation;
            } else if i >= 16000 {
                panic!("Too many ports reserved");
            }
            i += 1;
        };
        let ports = reservation.reserve_ports(10); // Try to reserve 10 from a range of 2

        // Should get at most 2 ports (the number available in the range)
        assert!(ports.len() <= 2);
        assert!(!ports.is_empty()); // Should still get some ports if available

        for port in ports {
            assert!((i..i + 2).contains(&port));
            assert!(reservation.is_reserved(port));
        }
    }

    #[test_log::test]
    #[serial]
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

    #[test_log::test]
    #[serial]
    fn test_inclusive_range() {
        let reservation = PortReservation::new(15000..=15010);
        let ports = reservation.reserve_ports(5);

        assert_eq!(ports.len(), 5);
        for port in ports {
            assert!((15000..=15010).contains(&port));
            assert!(reservation.is_reserved(port));
        }
    }

    #[test_log::test]
    #[serial]
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

    #[test_log::test]
    #[serial]
    fn test_concurrent_reservations() {
        use std::sync::Arc;
        use std::thread;

        let reservation = Arc::new(PortReservation::new(15000..15100));
        let mut handles = vec![];

        // Spawn 10 threads that each try to reserve 5 ports
        for _ in 0..10 {
            let reservation_clone = Arc::clone(&reservation);
            let handle = thread::spawn(move || reservation_clone.reserve_ports(5));
            handles.push(handle);
        }

        // Collect all reserved ports
        let mut all_ports = Vec::new();
        for handle in handles {
            let ports = handle.join().unwrap();
            all_ports.extend(ports);
        }

        // Verify no duplicates (each port should be reserved only once)
        let mut unique_ports = all_ports.clone();
        unique_ports.sort_unstable();
        unique_ports.dedup();
        assert_eq!(
            unique_ports.len(),
            all_ports.len(),
            "Concurrent reservations should not result in duplicate ports"
        );

        // Verify all ports are marked as reserved
        for port in &all_ports {
            assert!(
                reservation.is_reserved(*port),
                "Port {port} should be marked as reserved"
            );
        }
    }

    #[test_log::test]
    #[serial]
    fn test_concurrent_reserve_and_release() {
        use std::sync::Arc;
        use std::thread;

        let reservation = Arc::new(PortReservation::new(15000..15100));
        let mut handles = vec![];

        // Spawn threads that reserve and release ports concurrently
        for i in 0..5 {
            let reservation_clone = Arc::clone(&reservation);
            let handle = thread::spawn(move || {
                let ports = reservation_clone.reserve_ports(3);

                // Even-numbered threads release their ports
                if i % 2 == 0 {
                    reservation_clone.release_ports(ports.iter().copied());
                    vec![]
                } else {
                    ports
                }
            });
            handles.push(handle);
        }

        // Collect ports that were not released
        let mut remaining_ports = Vec::new();
        for handle in handles {
            let ports = handle.join().unwrap();
            remaining_ports.extend(ports);
        }

        // Verify that the remaining ports are still reserved
        for port in &remaining_ports {
            assert!(
                reservation.is_reserved(*port),
                "Port {port} should still be reserved"
            );
        }

        // Verify no duplicates among remaining ports
        let mut unique_ports = remaining_ports.clone();
        unique_ports.sort_unstable();
        unique_ports.dedup();
        assert_eq!(
            unique_ports.len(),
            remaining_ports.len(),
            "No duplicate ports should remain"
        );
    }

    #[test_log::test]
    fn test_is_reserved_non_reserved_port() {
        let reservation = PortReservation::new(15000..15100);

        // Check a port that was never reserved
        assert!(!reservation.is_reserved(15050));
    }

    #[test_log::test]
    fn test_release_non_reserved_port() {
        let reservation = PortReservation::new(15000..15100);

        // Releasing a port that was never reserved should not panic
        reservation.release_port(15050);
        assert!(!reservation.is_reserved(15050));
    }

    #[test_log::test]
    #[serial]
    fn test_reserve_port_returns_none_when_all_occupied() {
        // Create a reservation with a very limited range
        let reservation = PortReservation::new(15000..15002);

        // Reserve all available ports
        let port1 = reservation.reserve_port();
        let port2 = reservation.reserve_port();

        // At least one should succeed if ports are available
        assert!(port1.is_some() || port2.is_some());

        // If we got two ports, try to reserve a third - should fail eventually
        if port1.is_some() && port2.is_some() {
            // Try a few more times - system ports might not all be available
            let mut found_none = false;
            for _ in 0..10 {
                if reservation.reserve_port().is_none() {
                    found_none = true;
                    break;
                }
            }
            // With a very small range, we should eventually run out
            assert!(
                found_none,
                "Should run out of ports in a small range after multiple reservations"
            );
        }
    }

    #[test_log::test]
    #[serial]
    fn test_double_release() {
        let reservation = PortReservation::new(15000..15100);
        let port = reservation.reserve_port().unwrap();

        assert!(reservation.is_reserved(port));

        // Release once
        reservation.release_port(port);
        assert!(!reservation.is_reserved(port));

        // Release again - should not panic
        reservation.release_port(port);
        assert!(!reservation.is_reserved(port));
    }

    #[test_log::test]
    fn test_reserve_zero_ports() {
        let reservation = PortReservation::new(15000..15100);
        let ports = reservation.reserve_ports(0);

        assert!(ports.is_empty());
    }

    #[test_log::test]
    #[serial]
    fn test_release_empty_iterator() {
        let reservation = PortReservation::new(15000..15100);
        let port = reservation.reserve_port().unwrap();

        // Releasing an empty iterator should not affect existing reservations
        reservation.release_ports(std::iter::empty());

        assert!(reservation.is_reserved(port));
    }

    #[test_log::test]
    #[serial]
    fn test_is_reserved_port_outside_range() {
        let reservation = PortReservation::new(15000..15100);

        // Port outside range should never be reserved
        assert!(!reservation.is_reserved(14999));
        assert!(!reservation.is_reserved(15100));
        assert!(!reservation.is_reserved(20000));
    }

    #[test_log::test]
    #[serial]
    fn test_is_free_port_outside_range() {
        let reservation = PortReservation::new(15000..15100);

        // Port outside range should be considered "free" from reservation perspective
        assert!(reservation.is_free(14999));
        assert!(reservation.is_free(15100));
        assert!(reservation.is_free(20000));
    }

    #[test_log::test]
    #[serial]
    fn test_sequential_single_port_reservations() {
        let reservation = PortReservation::new(15000..15100);

        let port1 = reservation.reserve_port();
        let port2 = reservation.reserve_port();
        let port3 = reservation.reserve_port();

        assert!(port1.is_some());
        assert!(port2.is_some());
        assert!(port3.is_some());

        let port1 = port1.unwrap();
        let port2 = port2.unwrap();
        let port3 = port3.unwrap();

        // All ports should be different
        assert_ne!(port1, port2);
        assert_ne!(port2, port3);
        assert_ne!(port1, port3);

        // All ports should be reserved
        assert!(reservation.is_reserved(port1));
        assert!(reservation.is_reserved(port2));
        assert!(reservation.is_reserved(port3));
    }

    #[test_log::test]
    #[serial]
    fn test_release_specific_subset_of_ports() {
        let reservation = PortReservation::new(15000..15100);

        // Reserve 5 ports
        let ports = reservation.reserve_ports(5);
        assert_eq!(ports.len(), 5);

        // Release only the middle ports (indices 1, 2, 3)
        let middle_ports: Vec<_> = ports[1..4].to_vec();
        reservation.release_ports(middle_ports.iter().copied());

        // First and last ports should still be reserved
        assert!(reservation.is_reserved(ports[0]));
        assert!(reservation.is_reserved(ports[4]));

        // Middle ports should be released
        assert!(!reservation.is_reserved(ports[1]));
        assert!(!reservation.is_reserved(ports[2]));
        assert!(!reservation.is_reserved(ports[3]));
    }

    #[test_log::test]
    #[serial]
    fn test_reservation_is_free_helper() {
        use std::collections::BTreeSet;

        let mut reserved = BTreeSet::new();

        // Get a port that is actually free on the system
        let port = crate::pick_unused_port(15000..15100);
        if let Some(port) = port {
            // Port not in set and free on system should return true
            assert!(reservation_is_free(&reserved, port));

            // Add to reserved set
            reserved.insert(port);

            // Port in set should return false even if free on system
            assert!(!reservation_is_free(&reserved, port));
        }
    }
}
