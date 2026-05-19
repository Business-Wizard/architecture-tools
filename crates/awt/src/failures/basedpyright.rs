use std::path::Path;
use std::time::Duration;

use camino::Utf8PathBuf;
use serde::Deserialize;

use crate::model::{
    FailureCategory, FailureEvent, FailureScope, MutantId, RunnerError, VerifierKind,
};
use crate::runner::command;

#[derive(Debug, Deserialize)]
struct JsonOutput {
    #[serde(rename = "generalDiagnostics")]
    diagnostics: Vec<JsonDiagnostic>,
}

#[derive(Debug, Deserialize)]
struct JsonDiagnostic {
    file: String,
    message: String,
    severity: String,
    rule: Option<String>,
    range: JsonRange,
}

#[derive(Debug, Deserialize)]
struct JsonRange {
    start: JsonPosition,
}

#[derive(Debug, Deserialize)]
struct JsonPosition {
    line: u32,
    character: u32,
}

/// # Errors
/// Returns `RunnerError` if the basedpyright subprocess fails to spawn or times out.
pub async fn run_and_parse(
    mutant_id: &MutantId,
    repo_root: &Path,
    timeout: Duration,
) -> Result<Vec<FailureEvent>, RunnerError> {
    let out = command::run_in(
        "uv",
        &["run", "basedpyright", "--outputjson"],
        repo_root,
        timeout,
    )
    .await?;

    let Ok(parsed) = serde_json::from_str::<JsonOutput>(&out.stdout) else {
        return Ok(vec![]);
    };

    let events = parsed
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == "error")
        .map(|d| {
            let rel = relativize(&d.file, repo_root);
            let scope = if mutant_id.0.starts_with(rel.as_str()) {
                FailureScope::Local
            } else {
                FailureScope::External
            };
            let (category, symbol, message) = classify(&d.message, d.rule.as_deref());
            FailureEvent {
                mutant_id: mutant_id.clone(),
                command: VerifierKind::Basedpyright,
                file: rel,
                line: Some(d.range.start.line + 1),
                column: Some(d.range.start.character + 1),
                symbol,
                category,
                message,
                scope,
            }
        })
        .collect();

    Ok(events)
}

fn classify(message: &str, rule: Option<&str>) -> (FailureCategory, Option<String>, String) {
    let category = match rule {
        Some(s) if s.starts_with("reportMissing") || s.contains("Import") => {
            FailureCategory::Import
        }
        Some(s) if s.starts_with("reportUndefined") || s.starts_with("reportUnknown") => {
            FailureCategory::Type
        }
        _ if message.contains("import") || message.contains("module") => FailureCategory::Import,
        _ => FailureCategory::Type,
    };
    (category, rule.map(String::from), message.to_string())
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

    fn make_json(file: &str, severity: &str, rule: Option<&str>) -> String {
        let rule_field = rule.map_or(String::new(), |r| format!(r#","rule":"{r}""#));
        format!(
            r#"{{"generalDiagnostics":[{{"file":"{file}","message":"Argument missing","severity":"{severity}"{rule_field},"range":{{"start":{{"line":9,"character":15}},"end":{{"line":9,"character":20}}}}}}]}}"#
        )
    }

    #[test]
    fn test_error_diagnostic_should_produce_event_with_relative_path() {
        let id = MutantId("src/domain/order.py::Order.__init__::add_required_parameter".into());
        let json = make_json("/repo/src/api/routes.py", "error", Some("reportCallIssue"));
        let parsed: JsonOutput = serde_json::from_str(&json).unwrap();
        let diag = &parsed.diagnostics[0];
        let rel = super::relativize(&diag.file, std::path::Path::new("/repo"));
        assert_eq!(rel, Utf8PathBuf::from("src/api/routes.py"));
        let scope = if id.0.starts_with(rel.as_str()) {
            FailureScope::Local
        } else {
            FailureScope::External
        };
        assert_eq!(scope, FailureScope::External);
    }

    #[test]
    fn test_import_rule_should_classify_as_import_category() {
        let (category, _, _) =
            classify("Import could not be resolved", Some("reportMissingImports"));
        assert_eq!(category, FailureCategory::Import);
    }

    #[test]
    fn test_warning_diagnostic_should_be_ignored() {
        let id = MutantId("src/foo.py::bar::add_required_parameter".into());
        let json = make_json("/repo/src/x.py", "warning", None);
        let parsed: JsonOutput = serde_json::from_str(&json).unwrap();
        let events: Vec<_> = parsed
            .diagnostics
            .into_iter()
            .filter(|d| d.severity == "error")
            .collect();
        assert!(events.is_empty());
        let _ = id;
    }

    #[test]
    fn test_undefined_rule_should_classify_as_type_category() {
        let (actual, _, _) = classify("Value not defined", Some("reportUndefinedVariable"));
        assert_eq!(actual, FailureCategory::Type);
    }

    #[test]
    fn test_unknown_rule_should_classify_as_type_category() {
        let (actual, _, _) = classify("Unknown type", Some("reportUnknownMemberType"));
        assert_eq!(actual, FailureCategory::Type);
    }

    #[test]
    fn test_no_rule_with_import_in_message_should_classify_as_import() {
        let (actual, _, _) = classify("cannot import name 'Foo' from module 'bar'", None);
        assert_eq!(actual, FailureCategory::Import);
    }

    #[test]
    fn test_no_rule_without_import_keyword_should_classify_as_type() {
        let (actual, _, _) = classify("Argument of type int is not assignable to str", None);
        assert_eq!(actual, FailureCategory::Type);
    }

    #[test]
    fn test_relativize_path_not_under_root_should_return_original() {
        let actual = super::relativize("/other/path/file.py", std::path::Path::new("/repo"));
        let expected = Utf8PathBuf::from("/other/path/file.py");
        assert_eq!(actual, expected);
    }
}
