use std::collections::HashMap;
use std::path::Path;

use architecture_core::object_type::calculate_abstractness;
use camino::Utf8PathBuf;
use ignore::WalkBuilder;

use crate::graph::coupling_graph::FileRole;
use crate::python_ast::{self, ParsedFile};
use crate::repo;

#[derive(Debug)]
pub struct AbstractnessScore {
    pub value: Option<f64>,
}

impl AbstractnessScore {
    pub fn from_object_scores(scores: &[f32]) -> Self {
        if scores.is_empty() {
            return Self { value: None };
        }
        #[allow(clippy::cast_precision_loss)]
        let mean = scores.iter().copied().map(f64::from).sum::<f64>() / scores.len() as f64;
        Self { value: Some(mean) }
    }
}

pub struct AbstractnessMap {
    pub by_file: HashMap<Utf8PathBuf, AbstractnessScore>,
}

pub fn compute(repo_root: &Path, include_dirs: &[Utf8PathBuf]) -> AbstractnessMap {
    let mut by_file = HashMap::new();

    let repo_root_owned = repo_root.to_path_buf();
    let include_roots: Vec<std::path::PathBuf> = include_dirs
        .iter()
        .map(|d| repo_root.join(d.as_std_path()))
        .collect();
    let walker = WalkBuilder::new(repo_root)
        .hidden(false)
        .filter_entry(move |e| {
            let p = e.path();
            p == repo_root_owned || include_roots.iter().any(|root| p.starts_with(root))
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

        let Ok(source_bytes) = std::fs::read(path) else {
            continue;
        };

        let Some(parsed) = ParsedFile::parse(&source_bytes) else {
            continue;
        };

        let objects = python_ast::parse_objects(&parsed);
        let scores: Vec<f32> = objects
            .into_iter()
            .map(|obj| calculate_abstractness(obj).0)
            .collect();

        if !scores.is_empty() {
            by_file.insert(rel, AbstractnessScore::from_object_scores(&scores));
        }
    }

    AbstractnessMap { by_file }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_no_object_scores_should_have_none_value() {
        let score = AbstractnessScore::from_object_scores(&[]);
        assert!(score.value.is_none());
    }

    #[test]
    fn given_all_abstract_scores_should_have_value_one() {
        let score = AbstractnessScore::from_object_scores(&[1.0, 1.0]);
        assert_eq!(score.value, Some(1.0));
    }

    #[test]
    fn given_all_concrete_scores_should_have_value_zero() {
        let score = AbstractnessScore::from_object_scores(&[0.0, 0.0]);
        assert_eq!(score.value, Some(0.0));
    }

    #[test]
    fn given_mixed_scores_should_compute_mean() {
        let score = AbstractnessScore::from_object_scores(&[0.0, 1.0]);
        assert_eq!(score.value, Some(0.5));
    }
}
