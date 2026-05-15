use std::path::Path;
use std::time::Duration;

use camino::Utf8PathBuf;

use crate::model::{
    FailureCategory, FailureEvent, FailureScope, MutantId, RunnerError, VerifierKind,
};
use crate::runner::command;

pub fn run_and_parse(
    mutant_id: &MutantId,
    repo_root: &Path,
    timeout: Duration,
) -> Result<Vec<FailureEvent>, RunnerError> {
    let out = command::run_in("uv", &["run", "basedpyright"], repo_root, timeout)?;

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
//   /abs/path/file.py:10:4: error: message (reportXxx)
//   src/file.py:10:4: error: message
fn parse_line(mutant_id: &MutantId, line: &str, repo_root: &Path) -> Option<FailureEvent> {
    // Split on first ': error:' or ': warning:' or ': information:'
    let (location, rest) = split_at_severity(line)?;

    let mut loc_parts = location.splitn(3, ':');
    let raw_file = loc_parts.next()?.trim();
    let row: u32 = loc_parts.next()?.trim().parse().ok()?;
    let col: u32 = loc_parts.next()?.trim().parse().ok()?;

    let rel = relativize(raw_file, repo_root);
    let scope = if mutant_id.0.starts_with(rel.as_str()) {
        FailureScope::Local
    } else {
        FailureScope::External
    };

    let (category, symbol, message) = classify(rest);

    Some(FailureEvent {
        mutant_id: mutant_id.clone(),
        command: VerifierKind::Basedpyright,
        file: rel,
        line: Some(row),
        column: Some(col),
        symbol,
        category,
        message,
        scope,
    })
}

fn split_at_severity(line: &str) -> Option<(&str, &str)> {
    for marker in &[": error:", ": warning:", ": information:"] {
        if let Some(pos) = line.find(marker) {
            return Some((&line[..pos], &line[pos + marker.len()..]));
        }
    }
    None
}

fn classify(rest: &str) -> (FailureCategory, Option<String>, String) {
    let rest = rest.trim();

    // Extract rule name in parentheses at end: "message (reportXxx)"
    let (message, symbol) = if let Some(open) = rest.rfind('(') {
        if rest.ends_with(')') {
            let sym = rest[open + 1..rest.len() - 1].to_string();
            let msg = rest[..open].trim().to_string();
            (msg, Some(sym))
        } else {
            (rest.to_string(), None)
        }
    } else {
        (rest.to_string(), None)
    };

    let category = match symbol.as_deref() {
        Some(s) if s.starts_with("reportMissing") || s.contains("Import") => {
            FailureCategory::Import
        }
        Some(s) if s.starts_with("reportUndefined") || s.starts_with("reportUnknown") => {
            FailureCategory::Type
        }
        _ if message.contains("import") || message.contains("module") => FailureCategory::Import,
        _ => FailureCategory::Type,
    };

    (category, symbol, message)
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
    fn test_error_line_should_parse_file_line_col_message() {
        let id = MutantId("src/domain/order.py::Order.__init__::add_required_parameter".into());
        let line =
            "/repo/src/api/routes.py:42:8: error: Argument missing for parameter (reportCallIssue)";
        let actual = parse_line(&id, line, std::path::Path::new("/repo")).unwrap();
        let expected_file = Utf8PathBuf::from("src/api/routes.py");
        assert_eq!(actual.file, expected_file);
    }

    #[test]
    fn test_import_rule_should_classify_as_import_category() {
        let id = MutantId("src/foo.py::bar::add_required_parameter".into());
        let line = "/repo/src/x.py:1:1: error: Import could not be resolved (reportMissingImports)";
        let actual = parse_line(&id, line, std::path::Path::new("/repo")).unwrap();
        assert_eq!(actual.category, FailureCategory::Import);
    }

    #[test]
    fn test_non_error_line_should_return_none() {
        let id = MutantId("src/foo.py::bar::add_required_parameter".into());
        let line = "Found 3 errors in 2 files (checked 10 source files)";
        let actual = parse_line(&id, line, std::path::Path::new("/repo"));
        assert!(actual.is_none());
    }
}
