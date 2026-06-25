use thiserror::Error;

#[derive(Error, Debug)]
pub enum P2pError {
    #[error("Codec error: {0}")]
    Codec(String),
    #[error("Decompression failed: {0}")]
    Decompression(String),
    #[error("Frame size too large: {0}")]
    FrameSizeTooLarge(usize),
    #[error("Snappy decompressed size {0} exceeds limit {1}")]
    DecompressedSizeExceedsLimit(usize, usize),
    #[error("Header MAC mismatch")]
    HeaderMacMismatch,
    #[error("Frame MAC mismatch")]
    FrameMacMismatch,
}
