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
            Ok(VerifierStatus::Fail(collect_output(
                &out.stdout,
                &out.stderr,
            )))
        }
    }

    pub fn run_basedpyright(&self, repo: &Path) -> Result<VerifierStatus, RunnerError> {
        let out = command::run_in(
            "uv",
            &["run", "basedpyright", "--outputjson"],
            repo,
            self.timeout,
        )?;

        // basedpyright --outputjson exits 0 even with warnings; parse errors only
        let errors = extract_basedpyright_errors(&out.stdout);
        if errors.is_empty() && out.exit_code <= 1 {
            // exit 0 = clean, exit 1 = warnings only — both are baseline-pass
            Ok(VerifierStatus::Pass)
        } else if !errors.is_empty() {
            Ok(VerifierStatus::Fail(errors))
        } else {
            // exit >=2 means tool failure (not found, config error, etc.)
            Ok(VerifierStatus::Fail(collect_output(
                &out.stdout,
                &out.stderr,
            )))
        }
    }

    pub fn run_pytest(&self, repo: &Path) -> Result<VerifierStatus, RunnerError> {
        let out = command::run_in("uv", &["run", "pytest", "-q"], repo, self.timeout)?;
        if out.success() {
            Ok(VerifierStatus::Pass)
        } else {
            Ok(VerifierStatus::Fail(collect_output(
                &out.stdout,
                &out.stderr,
            )))
        }
    }
}

fn extract_basedpyright_errors(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return vec![];
    };
    let Some(diags) = value["generalDiagnostics"].as_array() else {
        return vec![];
    };
    diags
        .iter()
        .filter(|d| d["severity"].as_str() == Some("error"))
        .filter_map(|d| {
            let file = d["file"].as_str().unwrap_or("?");
            let msg = d["message"].as_str().unwrap_or("?");
            let line = d["range"]["start"]["line"].as_u64().unwrap_or(0) + 1;
            Some(format!("{file}:{line}: {msg}"))
        })
        .collect()
}

fn collect_output(stdout: &str, stderr: &str) -> Vec<String> {
    stdout
        .lines()
        .chain(stderr.lines())
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}
