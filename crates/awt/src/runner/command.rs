use std::path::Path;
use std::time::Duration;

use crate::model::RunnerError;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// # Errors
/// Returns `RunnerError::Io` if the process cannot be spawned, or `RunnerError::Timeout` if
/// the subprocess does not complete within `timeout`.
pub async fn run_in(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
) -> Result<CommandOutput, RunnerError> {
    let fut = tokio::process::Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env_remove("PYTHONPATH")
        .output();

    match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(output)) => Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        }),
        Ok(Err(e)) => Err(RunnerError::Io(e)),
        Err(_elapsed) => Err(RunnerError::Timeout(timeout.as_secs())),
    }
}
