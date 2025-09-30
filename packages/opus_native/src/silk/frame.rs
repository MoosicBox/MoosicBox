pub struct SilkFrame {
    pub frame_type: FrameType,
    pub vad_flag: bool,
    pub subframe_count: usize,
    pub subframe_gains: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Inactive,
    Unvoiced,
    Voiced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum QuantizationOffsetType {
    Low,
    High,
}
