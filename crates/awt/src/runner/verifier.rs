use std::path::Path;
use std::time::Duration;

use crate::model::{RunnerError, VerifierStatus};
use crate::runner::command;

pub struct VerifierSet {
    pub timeout: Duration,
}

impl VerifierSet {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub fn run_ruff(&self, repo: &Path) -> Result<VerifierStatus, RunnerError> {
        let out = command::run_in("uv", &["run", "ruff", "check", "."], repo, self.timeout)?;
        if out.success() {
            Ok(VerifierStatus::Pass)
        } else {
            let lines: Vec<String> = out
                .stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();
            Ok(VerifierStatus::Fail(lines))
        }
    }

    pub fn run_basedpyright(&self, repo: &Path) -> Result<VerifierStatus, RunnerError> {
        let out = command::run_in("uv", &["run", "basedpyright"], repo, self.timeout)?;
        if out.success() {
            Ok(VerifierStatus::Pass)
        } else {
            let lines: Vec<String> = out
                .stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();
            Ok(VerifierStatus::Fail(lines))
        }
    }

    pub fn run_pytest(&self, repo: &Path) -> Result<VerifierStatus, RunnerError> {
        let out = command::run_in("uv", &["run", "pytest", "-q"], repo, self.timeout)?;
        if out.success() {
            Ok(VerifierStatus::Pass)
        } else {
            let lines: Vec<String> = out
                .stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();
            Ok(VerifierStatus::Fail(lines))
        }
    }
}
