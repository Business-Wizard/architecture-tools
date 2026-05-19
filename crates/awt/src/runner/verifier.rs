use std::path::Path;
use std::time::Duration;

use crate::model::{RunnerError, VerifierStatus};
use crate::runner::command;

pub struct VerifierSet {
    pub timeout: Duration,
    pub include_dirs: Vec<String>,
}

impl VerifierSet {
    pub fn new(timeout_secs: u64, include_dirs: Vec<String>) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs),
            include_dirs,
        }
    }

    pub fn run_basedpyright(&self, repo: &Path) -> Result<VerifierStatus, RunnerError> {
        let out = command::run_in(
            "uv",
            &["run", "basedpyright", "--outputjson"],
            repo,
            self.timeout,
        )?;

        let include_roots: Vec<std::path::PathBuf> =
            self.include_dirs.iter().map(|d| repo.join(d)).collect();
        // basedpyright --outputjson exits 0 even with warnings; parse errors only
        let errors: Vec<String> = extract_basedpyright_errors(&out.stdout)
            .into_iter()
            .filter(|e| is_in_include_roots(e, &include_roots))
            .collect();
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
            Ok(VerifierStatus::Fail(extract_pytest_failures(
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
        .map(|d| {
            let file = d["file"].as_str().unwrap_or("?");
            let msg = d["message"].as_str().unwrap_or("?");
            let line = d["range"]["start"]["line"].as_u64().unwrap_or(0) + 1;
            format!("{file}:{line}: {msg}")
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

fn extract_pytest_failures(stdout: &str, stderr: &str) -> Vec<String> {
    stdout
        .lines()
        .chain(stderr.lines())
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("FAILED ")
                || t.starts_with("ERROR ")
                || t.starts_with("short test summary")
                || t.starts_with("= FAILURES")
                || t.starts_with("= ERRORS")
        })
        .map(String::from)
        .collect()
}

fn is_in_include_roots(line: &str, include_roots: &[std::path::PathBuf]) -> bool {
    include_roots
        .iter()
        .any(|root| line.starts_with(root.to_string_lossy().as_ref()))
}
