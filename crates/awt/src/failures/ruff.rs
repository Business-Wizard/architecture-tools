use std::path::Path;

use camino::Utf8PathBuf;
use serde::Deserialize;

use crate::model::RunnerError;
use crate::model::{FailureCategory, FailureEvent, FailureScope, MutantId, VerifierKind};
use crate::runner::command;

#[derive(Debug, Deserialize)]
struct RuffDiagnostic {
    filename: String,
    row: Option<u32>,
    col: Option<u32>,
    code: Option<String>,
    message: String,
}

pub fn run_and_parse(
    mutant_id: &MutantId,
    repo_root: &Path,
    timeout: std::time::Duration,
) -> Result<Vec<FailureEvent>, RunnerError> {
    let out = command::run_in(
        "uv",
        &["run", "ruff", "check", ".", "--output-format", "json"],
        repo_root,
        timeout,
    )?;

    if out.exit_code == 0 {
        return Ok(vec![]);
    }

    let diagnostics: Vec<RuffDiagnostic> = serde_json::from_str(&out.stdout).unwrap_or_default();

    let events = diagnostics
        .into_iter()
        .map(|d| {
            let rel = relativize(&d.filename, repo_root);
            let scope = if mutant_id.0.starts_with(rel.as_str()) {
                FailureScope::Local
            } else {
                FailureScope::External
            };
            FailureEvent {
                mutant_id: mutant_id.clone(),
                command: VerifierKind::Ruff,
                file: rel,
                line: d.row,
                column: d.col,
                symbol: d.code,
                category: FailureCategory::Lint,
                message: d.message,
                scope,
            }
        })
        .collect();

    Ok(events)
}

fn relativize(filename: &str, repo_root: &Path) -> Utf8PathBuf {
    let p = std::path::Path::new(filename);
    if let Ok(rel) = p.strip_prefix(repo_root) {
        return Utf8PathBuf::try_from(rel.to_path_buf())
            .unwrap_or_else(|_| Utf8PathBuf::from(filename));
    }
    // Retry after resolving symlinks (e.g. macOS /tmp → /private/tmp)
    if let (Ok(canon_p), Ok(canon_root)) = (p.canonicalize(), repo_root.canonicalize())
        && let Ok(rel) = canon_p.strip_prefix(&canon_root)
    {
        return Utf8PathBuf::try_from(rel.to_path_buf())
            .unwrap_or_else(|_| Utf8PathBuf::from(filename));
    }
    Utf8PathBuf::from(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_json_output_should_return_no_events() {
        let json = "[]";
        let diagnostics: Vec<RuffDiagnostic> = serde_json::from_str(json).unwrap();
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_ruff_diagnostic_should_parse_fields() {
        let json = r#"[{"filename":"/repo/src/foo.py","row":10,"col":4,"code":"F401","message":"unused import"}]"#;
        let diagnostics: Vec<RuffDiagnostic> = serde_json::from_str(json).unwrap();
        let actual = &diagnostics[0];
        let expected_message = "unused import";
        assert_eq!(actual.message, expected_message);
    }

    #[test]
    fn test_ruff_diagnostic_with_null_fields_should_parse_to_none() {
        let json = r#"[{"filename":"/repo/src/foo.py","row":null,"col":null,"code":null,"message":"bare error"}]"#;
        let diagnostics: Vec<RuffDiagnostic> = serde_json::from_str(json).unwrap();
        let actual = &diagnostics[0];
        assert_eq!(actual.row, None);
    }

    #[test]
    fn test_relativize_with_matching_prefix_should_strip_root() {
        let actual = super::relativize("/repo/src/foo.py", std::path::Path::new("/repo"));
        let expected = Utf8PathBuf::from("src/foo.py");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_relativize_with_non_matching_prefix_should_return_original() {
        let actual = super::relativize("/other/src/foo.py", std::path::Path::new("/repo"));
        let expected = Utf8PathBuf::from("/other/src/foo.py");
        assert_eq!(actual, expected);
    }
}
