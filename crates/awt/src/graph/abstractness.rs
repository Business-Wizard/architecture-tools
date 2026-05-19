use std::collections::HashMap;
use std::path::Path;

use camino::Utf8PathBuf;
use ignore::WalkBuilder;

use crate::graph::coupling_graph::FileRole;
use crate::python_ast::{self, ClassKind, ParsedFile};
use crate::repo;

#[derive(Debug)]
pub struct AbstractnessScore {
    #[expect(dead_code)]
    pub abstract_types: usize,
    #[expect(dead_code)]
    pub total_types: usize,
    pub value: Option<f64>,
}

impl AbstractnessScore {
    pub fn new(abstract_types: usize, total_types: usize) -> Self {
        let value = if total_types == 0 {
            None
        } else {
            #[allow(clippy::cast_precision_loss)]
            Some(abstract_types as f64 / total_types as f64)
        };

        Self {
            abstract_types,
            total_types,
            value,
        }
    }
}

pub struct AbstractnessMap {
    pub by_file: HashMap<Utf8PathBuf, AbstractnessScore>,
}

pub fn compute(repo_root: &Path, exclude_dirs: &[Utf8PathBuf]) -> AbstractnessMap {
    let mut by_file = HashMap::new();

    let exclude_dirs_owned: Vec<Utf8PathBuf> = exclude_dirs.to_vec();
    let walker = WalkBuilder::new(repo_root)
        .hidden(false)
        .filter_entry(move |e| {
            let name = e.file_name().to_string_lossy();
            !exclude_dirs_owned.iter().any(|ex| name == ex.as_str())
        })
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("py") {
            continue;
        }

        let rel = match path.strip_prefix(repo_root) {
            Ok(r) => repo::to_utf8(r),
            Err(_) => continue,
        };

        if let FileRole::Test = FileRole::from_path(&rel) {
            continue;
        }

        let Ok(source) = std::fs::read(path) else {
            continue;
        };

        let Some(parsed) = ParsedFile::parse(&source) else {
            continue;
        };

        let classes = python_ast::find_classes(&parsed);

        let abstract_count = classes
            .iter()
            .filter(|c| c.kind == ClassKind::Abstract || c.kind == ClassKind::Protocol)
            .count();

        let total_count = classes.len();

        if total_count > 0 {
            by_file.insert(rel, AbstractnessScore::new(abstract_count, total_count));
        }
    }

    AbstractnessMap { by_file }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_with_no_types_should_have_none_value() {
        let score = AbstractnessScore::new(0, 0);
        assert!(score.value.is_none());
    }

    #[test]
    fn test_score_with_all_abstract_should_have_value_one() {
        let score = AbstractnessScore::new(2, 2);
        assert_eq!(score.value, Some(1.0));
    }

    #[test]
    fn test_score_with_no_abstract_should_have_value_zero() {
        let score = AbstractnessScore::new(0, 3);
        assert_eq!(score.value, Some(0.0));
    }

    #[test]
    fn test_score_with_mixed_should_compute_ratio() {
        let score = AbstractnessScore::new(1, 4);
        assert_eq!(score.value, Some(0.25));
    }
}
