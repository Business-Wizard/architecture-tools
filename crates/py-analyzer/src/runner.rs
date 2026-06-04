use std::path::Path;
use std::time::Duration;

use crate::error::InspectorError;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

pub async fn run_in(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
) -> Result<CommandOutput, InspectorError> {
    let fut = tokio::process::Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output();

    match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(output)) => Ok(CommandOutput {
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        }),
        Ok(Err(e)) => Err(InspectorError::Io(e)),
        Err(_elapsed) => Err(InspectorError::Timeout(timeout.as_secs())),
    }
}
