use thiserror::Error;

#[derive(Debug, Error)]
pub enum InspectorError {
    #[error("subprocess spawn failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("subprocess timed out after {0}s")]
    Timeout(u64),

    #[error("pyreverse failed (exit {code}): {stderr}")]
    PyreverseFailed { code: i32, stderr: String },

    #[error("no packages_*.dot file found in pyreverse output")]
    NoDotFile,
}
