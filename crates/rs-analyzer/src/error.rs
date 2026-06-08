use thiserror::Error;

#[derive(Debug, Error)]
pub enum InspectorError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse Rust source")]
    ParseFailed,
}
