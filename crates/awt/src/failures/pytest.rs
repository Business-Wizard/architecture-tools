use std::path::Path;
use std::time::Duration;

use crate::failures::common::relativize;
use crate::model::{
    FailureCategory, FailureEvent, FailureScope, MutantId, RunnerError, VerifierKind,
};
use crate::runner::command;

/// # Errors
/// Returns `RunnerError` if the pytest subprocess fails to spawn or times out.
pub async fn run_and_parse(
    mutant_id: &MutantId,
    repo_root: &Path,
    timeout: Duration,
) -> Result<Vec<FailureEvent>, RunnerError> {
    let out = command::run_in("uv", &["run", "pytest", "-q"], repo_root, timeout).await?;

    if out.exit_code == 0 {
        return Ok(vec![]);
    }

    Ok(parse_text_output(mutant_id, &out.stdout, repo_root))
}

fn parse_text_output(mutant_id: &MutantId, text: &str, repo_root: &Path) -> Vec<FailureEvent> {
    text.lines()
        .filter_map(|line| parse_line(mutant_id, line, repo_root))
        .collect()
}

// Parses lines like:
//   FAILED tests/test_order.py::TestOrder::test_create - AssertionError: ...
//   ERROR tests/test_order.py::TestOrder::test_create - ImportError: ...
fn parse_line(mutant_id: &MutantId, line: &str, repo_root: &Path) -> Option<FailureEvent> {
    let (prefix, rest) = if let Some(rest) = line.strip_prefix("FAILED ") {
        ("FAILED", rest)
    } else if let Some(rest) = line.strip_prefix("ERROR ") {
        ("ERROR", rest)
    } else {
        return None;
    };

    // Split node_id from message: "tests/foo.py::bar - ExcType: msg"
    let (node_id, message) = rest.split_once(" - ").map_or((rest, ""), |(n, m)| (n, m));

    // Extract file from node_id (everything before first ::)
    let raw_file = node_id.split("::").next()?;
    let rel = relativize(raw_file, repo_root);
    let scope = if mutant_id.0.starts_with(rel.as_str()) {
        FailureScope::Local
    } else {
        FailureScope::External
    };

    let category = classify_pytest(prefix, message);

    Some(FailureEvent {
        mutant_id: mutant_id.clone(),
        command: VerifierKind::Pytest,
        file: rel,
        line: None,
        column: None,
        symbol: Some(node_id.to_string()),
        category,
        message: message.to_string(),
        scope,
    })
}

fn classify_pytest(prefix: &str, message: &str) -> FailureCategory {
    if prefix == "ERROR" {
        if message.contains("ImportError") || message.contains("ModuleNotFoundError") {
            return FailureCategory::Import;
        }
        return FailureCategory::TestCollection;
    }
    if message.contains("TypeError") {
        return FailureCategory::Type;
    }
    if message.contains("ImportError") || message.contains("ModuleNotFoundError") {
        return FailureCategory::Import;
    }
    FailureCategory::TestAssertion
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;

    #[test]
    fn test_failed_line_should_parse_file_and_category() {
        let id = MutantId("src/domain/order.py::Order.__init__::add_required_parameter".into());
        let line =
            "FAILED tests/test_order.py::TestOrder::test_create - AssertionError: assert 1 == 2";
        let actual = parse_line(&id, line, std::path::Path::new("/repo")).unwrap();
        let expected_file = Utf8PathBuf::from("tests/test_order.py");
        assert_eq!(actual.file, expected_file);
    }

    #[test]
    fn test_import_error_should_classify_as_import_category() {
        let id = MutantId("src/foo.py::bar::add_required_parameter".into());
        let line = "ERROR tests/test_order.py::TestOrder::test_create - ImportError: cannot import";
        let actual = parse_line(&id, line, std::path::Path::new("/repo")).unwrap();
        assert_eq!(actual.category, FailureCategory::Import);
    }

    #[test]
    fn test_non_failure_line_should_return_none() {
        let id = MutantId("src/foo.py::bar::add_required_parameter".into());
        let line = "1 passed in 0.04s";
        let actual = parse_line(&id, line, std::path::Path::new("/repo"));
        assert!(actual.is_none());
    }

    #[test]
    fn test_failed_line_should_produce_external_scope_for_test_file() {
        let id = MutantId("src/domain/order.py::Order.__init__::add_required_parameter".into());
        let line = "FAILED tests/test_order.py::TestOrder::test_create - AssertionError: x";
        let actual = parse_line(&id, line, std::path::Path::new("/repo")).unwrap();
        assert_eq!(actual.scope, FailureScope::External);
    }

    #[test]
    fn test_error_module_not_found_should_classify_as_import() {
        let actual = classify_pytest("ERROR", "ModuleNotFoundError: No module named 'foo'");
        assert_eq!(actual, FailureCategory::Import);
    }

    #[test]
    fn test_error_without_import_should_classify_as_test_collection() {
        let actual = classify_pytest("ERROR", "fixture 'db' not found");
        assert_eq!(actual, FailureCategory::TestCollection);
    }

    #[test]
    fn test_failed_with_type_error_should_classify_as_type() {
        let actual = classify_pytest(
            "FAILED",
            "TypeError: foo() takes 1 positional argument but 2 given",
        );
        assert_eq!(actual, FailureCategory::Type);
    }
}
