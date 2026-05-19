use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

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

pub fn run_in(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
) -> Result<CommandOutput, RunnerError> {
    let start = Instant::now();

    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env_remove("PYTHONPATH")
        .output()
        .map_err(RunnerError::Io)?;

    if start.elapsed() >= timeout {
        return Err(RunnerError::Timeout(timeout.as_secs()));
    }

    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}
