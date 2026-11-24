//! UUID value and compatibility conversions.

use std::fmt;
use std::str::FromStr;

/// A universally unique identifier (UUID).
///
/// Stored in network byte order. The `uuid-compat` feature retains a borrowed
/// external representation for callers of the const `inner` API.
///
/// # Examples
///
/// ```
/// use switchy_uuid::Uuid;
///
/// // Generate a new random UUID
/// # #[cfg(any(feature = "uuid", feature = "simulator"))]
/// # {
/// let id = Uuid::new_v4();
///
/// // Convert to string
/// let id_string = id.to_string();
///
/// // Parse from string
/// let parsed: Uuid = id_string.parse().unwrap();
/// assert_eq!(id, parsed);
/// # }
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Uuid(
    #[cfg(feature = "uuid-compat")] uuid::Uuid,
    #[cfg(not(feature = "uuid-compat"))] [u8; 16],
);

impl Uuid {
    /// The number of bytes in a UUID.
    pub const SIZE: usize = 16;

    /// Creates a UUID from the high and low halves in network byte order.
    #[must_use]
    pub const fn from_u64_pair(high: u64, low: u64) -> Self {
        Self::from_u128(((high as u128) << 64) | low as u128)
    }

    /// Returns the high and low halves in network byte order.
    #[must_use]
    pub const fn as_u64_pair(&self) -> (u64, u64) {
        let (halves, _) = self.as_bytes().as_chunks::<8>();
        (u64::from_be_bytes(halves[0]), u64::from_be_bytes(halves[1]))
    }

    /// Builds a version 4 UUID from caller-supplied bytes, without drawing entropy.
    /// Only the version and RFC variant bits are overwritten.
    #[must_use]
    pub const fn from_random_bytes(mut bytes: [u8; 16]) -> Self {
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Self::from_bytes(bytes)
    }

    /// Creates a UUID from a 128-bit value.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::from_u128(0x550e8400_e29b_41d4_a716_446655440000);
    /// ```
    #[must_use]
    pub const fn from_u128(v: u128) -> Self {
        Self::from_bytes(v.to_be_bytes())
    }

    /// Creates a UUID from a 128-bit value in little-endian byte order.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::from_u128_le(0x550e8400_e29b_41d4_a716_446655440000);
    /// ```
    #[must_use]
    pub const fn from_u128_le(v: u128) -> Self {
        Self::from_bytes(v.to_le_bytes())
    }

    /// Returns the UUID as a 128-bit value.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let v: u128 = 0x550e_8400_e29b_41d4_a716_4466_5544_0000;
    /// let uuid = Uuid::from_u128(v);
    /// assert_eq!(uuid.as_u128(), v);
    /// ```
    #[must_use]
    pub const fn as_u128(&self) -> u128 {
        u128::from_be_bytes(*self.as_bytes())
    }

    /// Returns the UUID as a 128-bit value in little-endian byte order.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::from_u128(0x550e8400_e29b_41d4_a716_446655440000);
    /// let le_value = uuid.as_u128_le();
    /// ```
    #[must_use]
    pub const fn as_u128_le(&self) -> u128 {
        u128::from_le_bytes(*self.as_bytes())
    }

    /// Creates a UUID from 16 bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let bytes = [0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4,
    ///              0xa7, 0x16, 0x44, 0x66, 0x55, 0x44, 0x00, 0x00];
    /// let uuid = Uuid::from_bytes(bytes);
    /// ```
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        #[cfg(feature = "uuid-compat")]
        let bytes = uuid::Uuid::from_bytes(bytes);
        Self(bytes)
    }

    /// Creates a UUID from a byte slice.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if `slice` does not contain exactly 16 bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let bytes = [0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4,
    ///              0xa7, 0x16, 0x44, 0x66, 0x55, 0x44, 0x00, 0x00];
    /// let uuid = Uuid::from_slice(&bytes).unwrap();
    /// ```
    pub fn from_slice(slice: &[u8]) -> Result<Self, ParseError> {
        slice
            .try_into()
            .map(Self::from_bytes)
            .map_err(|_| ParseError("expected 16 bytes".into()))
    }

    /// Returns the bytes of the UUID.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::nil();
    /// assert_eq!(uuid.as_bytes(), &[0u8; 16]);
    /// ```
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        #[cfg(feature = "uuid-compat")]
        {
            self.0.as_bytes()
        }
        #[cfg(not(feature = "uuid-compat"))]
        {
            &self.0
        }
    }

    /// Returns the bytes of the UUID as an owned array.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::nil();
    /// let bytes: [u8; 16] = uuid.into_bytes();
    /// assert_eq!(bytes, [0u8; 16]);
    /// ```
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        *self.as_bytes()
    }

    /// Creates a nil UUID (all zeros).
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let nil = Uuid::nil();
    /// assert!(nil.is_nil());
    /// ```
    #[must_use]
    pub const fn nil() -> Self {
        Self::from_bytes([0; 16])
    }

    /// Returns `true` if this is a nil UUID (all zeros).
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// # #[cfg(any(feature = "uuid", feature = "simulator"))]
    /// # {
    /// assert!(Uuid::nil().is_nil());
    /// assert!(!Uuid::new_v4().is_nil());
    /// # }
    /// ```
    #[must_use]
    pub const fn is_nil(&self) -> bool {
        self.as_u128() == 0
    }

    /// Creates a max UUID (all ones).
    ///
    /// The max UUID is `ffffffff-ffff-ffff-ffff-ffffffffffff`.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let max = Uuid::max();
    /// assert!(max.is_max());
    /// assert_eq!(max.as_bytes(), &[0xffu8; 16]);
    /// ```
    #[must_use]
    pub const fn max() -> Self {
        Self::from_bytes([0xff; 16])
    }

    /// Returns `true` if this is a max UUID (all ones).
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// assert!(Uuid::max().is_max());
    /// assert!(!Uuid::nil().is_max());
    /// ```
    #[must_use]
    pub const fn is_max(&self) -> bool {
        self.as_u128() == u128::MAX
    }

    /// Returns the version number of the UUID.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// # #[cfg(any(feature = "uuid", feature = "simulator"))]
    /// # {
    /// let uuid = Uuid::new_v4();
    /// assert_eq!(uuid.get_version_num(), 4);
    /// # }
    /// ```
    #[must_use]
    pub const fn get_version_num(&self) -> usize {
        (self.as_bytes()[6] >> 4) as usize
    }

    /// Parses a UUID from a string.
    ///
    /// # Errors
    ///
    /// * Returns [`ParseError`] if `input` is not a valid UUID string in any
    ///   supported format.
    ///
    /// # Accepted formats
    ///
    /// * Simple: `550e8400e29b41d4a716446655440000`
    /// * Hyphenated: `550e8400-e29b-41d4-a716-446655440000`
    /// * Braced: `{550e8400-e29b-41d4-a716-446655440000}`
    /// * URN: `urn:uuid:550e8400-e29b-41d4-a716-446655440000`
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    /// ```
    pub fn parse_str(input: &str) -> Result<Self, ParseError> {
        crate::text::parse(input)
            .map(Self::from_bytes)
            .ok_or_else(|| ParseError("invalid UUID spelling".into()))
    }

    /// Returns a reference to the inner `uuid::Uuid`.
    ///
    /// This method provides access to the underlying UUID type for interoperability
    /// with code that requires the standard `uuid::Uuid` type.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::nil();
    /// let inner: &uuid::Uuid = uuid.inner();
    /// ```
    #[must_use]
    #[cfg(feature = "uuid-compat")]
    pub const fn inner(&self) -> &uuid::Uuid {
        &self.0
    }

    /// Consumes the wrapper and returns the inner `uuid::Uuid`.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::nil();
    /// let inner: uuid::Uuid = uuid.into_inner();
    /// ```
    #[must_use]
    #[cfg(feature = "uuid-compat")]
    pub const fn into_inner(self) -> uuid::Uuid {
        self.0
    }

    /// Creates a wrapper from a `uuid::Uuid`.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let inner = uuid::Uuid::nil();
    /// let wrapped = Uuid::from_inner(inner);
    /// ```
    #[must_use]
    #[cfg(feature = "uuid-compat")]
    pub const fn from_inner(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the UUID as a hyphenated string.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::nil();
    /// assert_eq!(uuid.hyphenated(), "00000000-0000-0000-0000-000000000000");
    /// ```
    #[must_use]
    pub fn hyphenated(&self) -> String {
        self.to_string()
    }

    /// Returns the UUID as a simple (non-hyphenated) string.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::nil();
    /// assert_eq!(uuid.simple(), "00000000000000000000000000000000");
    /// ```
    #[must_use]
    pub fn simple(&self) -> String {
        format!("{:032x}", self.as_u128())
    }

    /// Returns the UUID as a URN string.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::nil();
    /// assert_eq!(uuid.urn(), "urn:uuid:00000000-0000-0000-0000-000000000000");
    /// ```
    #[must_use]
    pub fn urn(&self) -> String {
        format!("urn:uuid:{self}")
    }

    /// Returns the UUID as a braced string.
    ///
    /// # Examples
    ///
    /// ```
    /// use switchy_uuid::Uuid;
    ///
    /// let uuid = Uuid::nil();
    /// assert_eq!(uuid.braced(), "{00000000-0000-0000-0000-000000000000}");
    /// ```
    #[must_use]
    pub fn braced(&self) -> String {
        format!("{{{self}}}")
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::text::display(self.as_bytes(), f)
    }
}

impl std::hash::Hash for Uuid {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write(self.as_bytes());
    }
}

impl FromStr for Uuid {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_str(s)
    }
}

impl Default for Uuid {
    fn default() -> Self {
        Self::nil()
    }
}

#[cfg(feature = "uuid-compat")]
impl From<uuid::Uuid> for Uuid {
    fn from(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }
}

#[cfg(feature = "uuid-compat")]
impl From<Uuid> for uuid::Uuid {
    fn from(uuid: Uuid) -> Self {
        uuid.0
    }
}

impl From<[u8; 16]> for Uuid {
    fn from(bytes: [u8; 16]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<Uuid> for [u8; 16] {
    fn from(uuid: Uuid) -> Self {
        uuid.into_bytes()
    }
}

impl From<u128> for Uuid {
    fn from(v: u128) -> Self {
        Self::from_u128(v)
    }
}

impl From<Uuid> for u128 {
    fn from(uuid: Uuid) -> Self {
        uuid.as_u128()
    }
}

impl AsRef<[u8]> for Uuid {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

#[cfg(feature = "uuid-compat")]
impl AsRef<uuid::Uuid> for Uuid {
    fn as_ref(&self) -> &uuid::Uuid {
        &self.0
    }
}

/// An error that occurred while parsing a UUID string.
///
/// This error is returned when attempting to parse an invalid UUID string
/// via [`Uuid::parse_str`] or the [`FromStr`] implementation.
///
/// # Examples
///
/// ```
/// use switchy_uuid::Uuid;
///
/// let err = Uuid::parse_str("not-a-uuid").unwrap_err();
/// assert!(err.to_string().contains("invalid UUID"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid UUID: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_halves_and_v4_mask() {
        let halves = (0x0011_2233_4455_6677, 0xcc99_aabb_ccdd_eeff);
        let id = Uuid::from_u64_pair(halves.0, halves.1);
        assert_eq!(id.as_u64_pair(), halves);
        assert_eq!(id.to_string(), "00112233-4455-6677-cc99-aabbccddeeff");
        assert_eq!(
            Uuid::from_random_bytes(*id.as_bytes()).to_string(),
            "00112233-4455-4677-8c99-aabbccddeeff"
        );
    }

    #[test]
    fn test_nil() {
        let nil = Uuid::nil();
        assert!(nil.is_nil());
        assert_eq!(nil.as_bytes(), &[0u8; 16]);
        assert_eq!(nil.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn test_max() {
        let max = Uuid::max();
        assert!(max.is_max());
        assert_eq!(max.as_bytes(), &[0xffu8; 16]);
    }

    #[test]
    fn test_from_bytes() {
        let bytes = [
            0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4, 0xa7, 0x16, 0x44, 0x66, 0x55, 0x44,
            0x00, 0x00,
        ];
        let uuid = Uuid::from_bytes(bytes);
        assert_eq!(uuid.as_bytes(), &bytes);
    }

    #[test]
    fn test_parse_str() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_parse_str_simple() {
        let uuid = Uuid::parse_str("550e8400e29b41d4a716446655440000").unwrap();
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_parse_str_braced() {
        let uuid = Uuid::parse_str("{550e8400-e29b-41d4-a716-446655440000}").unwrap();
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_parse_str_urn() {
        let uuid = Uuid::parse_str("urn:uuid:550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_parse_str_invalid() {
        assert!(Uuid::parse_str("not-a-uuid").is_err());
        assert!(Uuid::parse_str("").is_err());
    }

    #[test]
    fn test_from_str() {
        let uuid: Uuid = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_format_methods() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(uuid.hyphenated(), "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(uuid.simple(), "550e8400e29b41d4a716446655440000");
        assert_eq!(uuid.urn(), "urn:uuid:550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(uuid.braced(), "{550e8400-e29b-41d4-a716-446655440000}");
    }

    #[test]
    fn test_from_u128() {
        let v: u128 = 0x550e_8400_e29b_41d4_a716_4466_5544_0000;
        let uuid = Uuid::from_u128(v);
        assert_eq!(uuid.as_u128(), v);
    }

    #[test]
    fn test_equality() {
        let uuid1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let uuid2 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let uuid3 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();

        assert_eq!(uuid1, uuid2);
        assert_ne!(uuid1, uuid3);
    }

    #[test]
    fn test_ordering() {
        let uuid1 = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let uuid2 = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();

        assert!(uuid1 < uuid2);
    }

    #[test]
    fn test_hash() {
        use std::collections::BTreeSet;

        let mut set = BTreeSet::new();
        let uuid1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let uuid2 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();

        set.insert(uuid1);
        assert!(!set.insert(uuid2)); // Should return false as it's a duplicate
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_default() {
        let uuid = Uuid::default();
        assert!(uuid.is_nil());
    }

    #[test]
    #[cfg(feature = "uuid-compat")]
    fn test_from_inner() {
        let inner = uuid::Uuid::nil();
        let wrapped = Uuid::from_inner(inner);
        assert!(wrapped.is_nil());
        assert_eq!(wrapped.into_inner(), inner);
    }

    #[test]
    fn test_conversions() {
        let bytes = [
            0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4, 0xa7, 0x16, 0x44, 0x66, 0x55, 0x44,
            0x00, 0x00,
        ];

        // From bytes
        let uuid: Uuid = bytes.into();
        assert_eq!(uuid.as_bytes(), &bytes);

        // To bytes
        let result: [u8; 16] = uuid.into();
        assert_eq!(result, bytes);
    }

    #[test]
    fn test_from_slice() {
        let bytes = [
            0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4, 0xa7, 0x16, 0x44, 0x66, 0x55, 0x44,
            0x00, 0x00,
        ];
        let uuid = Uuid::from_slice(&bytes).unwrap();
        assert_eq!(uuid.as_bytes(), &bytes);

        // Invalid slice length
        assert!(Uuid::from_slice(&[0u8; 15]).is_err());
        assert!(Uuid::from_slice(&[0u8; 17]).is_err());
    }
}
