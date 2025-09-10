use sha2::Sha256;

/// Trait for types that can contribute to a checksum digest
pub trait Digest {
    fn update_digest(&self, hasher: &mut Sha256);
}
