use std::path::Path;
use std::time::Duration;

use crate::error::InspectorError;
use crate::model::ModuleDep;
use crate::runner;

pub async fn extract_module_deps(
    package_path: &Path,
    timeout: Duration,
) -> Result<Vec<ModuleDep>, InspectorError> {
    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path();
    let path_str = package_path.to_string_lossy();

    let output = runner::run_in(
        "uv",
        &[
            "run",
            "pyreverse",
            "-o",
            "dot",
            "-d",
            &tmp_path.to_string_lossy(),
            &path_str,
        ],
        package_path,
        timeout,
    )
    .await?;

    if !output.success() {
        return Err(InspectorError::PyreverseFailed {
            code: output.exit_code,
            stderr: output.stderr,
        });
    }

    let dot_file = find_packages_dot(tmp_path)?;
    let content = std::fs::read_to_string(&dot_file)?;
    Ok(parse_dot_edges(&content))
}

fn find_packages_dot(dir: &Path) -> Result<std::path::PathBuf, InspectorError> {
    std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with("packages_")
                    && std::path::Path::new(n)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("dot"))
            })
        })
        .ok_or(InspectorError::NoDotFile)
}

/// Extract `"A" -> "B"` edges from a DOT file, ignoring graph boilerplate.
fn parse_dot_edges(dot: &str) -> Vec<ModuleDep> {
    dot.lines()
        .filter_map(|line| {
            let line = line.trim();
            // Match: "module.a" -> "module.b" [optional attrs]
            let (lhs, rest) = line.split_once("->")?;
            let from = extract_dot_label(lhs.trim())?;
            let to = extract_dot_label(rest.trim())?;
            Some(ModuleDep { from, to })
        })
        .collect()
}

fn extract_dot_label(s: &str) -> Option<String> {
    // DOT node labels are quoted: "module.name" or unquoted identifiers
    if s.starts_with('"') {
        let inner = s.trim_start_matches('"');
        // Strip trailing quote and anything after (e.g. " [...]")
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        // Unquoted: take until whitespace or bracket
        let end = s
            .find(|c: char| c.is_whitespace() || c == '[')
            .unwrap_or(s.len());
        let label = &s[..end];
        if label.is_empty() {
            None
        } else {
            Some(label.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dot_edges_should_extract_quoted_module_names() {
        let dot = r#"
digraph packages {
  "myapp.domain" -> "myapp.usecases"
  "myapp.usecases" -> "myapp.domain"
  "myapp.views" -> "myapp.presenter"
}
"#;
        let actual: Vec<(String, String)> = parse_dot_edges(dot)
            .into_iter()
            .map(|d| (d.from, d.to))
            .collect();
        let expected = [
            ("myapp.domain", "myapp.usecases"),
            ("myapp.usecases", "myapp.domain"),
            ("myapp.views", "myapp.presenter"),
        ]
        .map(|(f, t)| (f.to_string(), t.to_string()));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_dot_edges_should_skip_non_edge_lines() {
        let dot = r#"
digraph packages {
  graph [rankdir=BT]
  node [shape=box]
  "a" -> "b"
}
"#;
        let actual: Vec<(String, String)> = parse_dot_edges(dot)
            .into_iter()
            .map(|d| (d.from, d.to))
            .collect();
        assert_eq!(actual, [("a".to_string(), "b".to_string())]);
    }
}
