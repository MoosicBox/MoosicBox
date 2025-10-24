use sha2::Sha256;

/// Trait for types that can contribute to a checksum digest
pub trait Digest {
    /// Update the hasher with this value's contribution to the checksum
    ///
    /// This method feeds data into the SHA-256 hasher in a deterministic way,
    /// ensuring that the same value always produces the same checksum.
    fn update_digest(&self, hasher: &mut Sha256);
}
