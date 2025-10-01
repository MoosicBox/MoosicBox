#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use crate::silk::decoder::Bandwidth;

pub const LCG_SEED_PDF: &[u8] = &[192, 128, 64, 0];

#[must_use]
pub const fn get_shell_block_count(bandwidth: Bandwidth, frame_size_ms: u8) -> Option<usize> {
    match (bandwidth, frame_size_ms) {
        (Bandwidth::Narrowband, 10) => Some(5),
        (Bandwidth::Narrowband, 20) | (Bandwidth::Wideband, 10) => Some(10),
        (Bandwidth::Mediumband, 10) => Some(8),
        (Bandwidth::Mediumband, 20) => Some(15),
        (Bandwidth::Wideband, 20) => Some(20),
        _ => None,
    }
}
