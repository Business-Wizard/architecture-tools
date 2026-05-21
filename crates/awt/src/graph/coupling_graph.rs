use std::collections::HashMap;

use camino::Utf8PathBuf;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::model::MutantResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRole {
    Source,
    Test,
}

impl FileRole {
    pub fn from_path(path: &Utf8PathBuf) -> Self {
        let s = path.as_str();
        if s.contains("/tests/")
            || s.contains("/test_")
            || s.ends_with("_test.py")
            || s.starts_with("tests/")
            || s.starts_with("test_")
        {
            FileRole::Test
        } else {
            FileRole::Source
        }
    }
}

#[derive(Debug, Clone)]
pub struct CouplingNode {
    pub path: Utf8PathBuf,
    pub role: FileRole,
}

#[derive(Debug, Clone)]
pub struct CouplingEdge {
    pub failure_count: usize,
}

pub type CouplingGraph = DiGraph<CouplingNode, CouplingEdge>;

pub struct GraphIndex {
    pub graph: CouplingGraph,
}

pub(crate) fn resolve_source_module(
    test_path: &Utf8PathBuf,
    known_source_files: &[Utf8PathBuf],
) -> Option<Utf8PathBuf> {
    if FileRole::from_path(test_path) == FileRole::Source {
        return None;
    }
    let stem = test_path.file_stem()?;
    let candidate_stem = if let Some(s) = stem.strip_prefix("test_") {
        s.to_string()
    } else if let Some(s) = stem.strip_suffix("_test") {
        s.to_string()
    } else {
        return None;
    };
    let target_filename = format!("{candidate_stem}.py");
    let matches: Vec<&Utf8PathBuf> = known_source_files
        .iter()
        .filter(|f| f.file_name() == Some(target_filename.as_str()))
        .collect();
    match matches.len() {
        0 => None,
        1 => Some(matches[0].clone()),
        _ => {
            let test_parent = test_path.parent().map_or("", camino::Utf8Path::as_str);
            matches
                .into_iter()
                .max_by(|a, b| {
                    let common_a = common_prefix_len(
                        test_parent,
                        a.parent().map_or("", camino::Utf8Path::as_str),
                    );
                    let common_b = common_prefix_len(
                        test_parent,
                        b.parent().map_or("", camino::Utf8Path::as_str),
                    );
                    common_a.cmp(&common_b).then(a.as_str().cmp(b.as_str()))
                })
                .cloned()
        }
    }
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

impl GraphIndex {
    pub fn build(results: &[MutantResult], known_source_files: &[Utf8PathBuf]) -> Self {
        let mut graph = CouplingGraph::new();
        let mut node_map: HashMap<Utf8PathBuf, NodeIndex> = HashMap::new();

        let get_or_insert = |g: &mut CouplingGraph,
                             map: &mut HashMap<Utf8PathBuf, NodeIndex>,
                             path: Utf8PathBuf| {
            if let Some(&idx) = map.get(&path) {
                return idx;
            }
            let role = FileRole::from_path(&path);
            let idx = g.add_node(CouplingNode {
                path: path.clone(),
                role,
            });
            map.insert(path, idx);
            idx
        };

        for result in results {
            if result.external_failures.is_empty() {
                continue;
            }

            let src_idx = get_or_insert(&mut graph, &mut node_map, result.candidate.file.clone());

            let mut affected: HashMap<Utf8PathBuf, usize> = HashMap::new();
            for f in &result.external_failures {
                *affected.entry(f.file.clone()).or_insert(0) += 1;
            }

            for (affected_file, count) in affected {
                let resolved = if FileRole::from_path(&affected_file) == FileRole::Test {
                    match resolve_source_module(&affected_file, known_source_files) {
                        Some(src) => src,
                        None => continue,
                    }
                } else {
                    affected_file
                };

                if resolved == result.candidate.file {
                    continue;
                }

                let dst_idx = get_or_insert(&mut graph, &mut node_map, resolved);
                if let Some(e) = graph.find_edge(src_idx, dst_idx) {
                    graph[e].failure_count += count;
                } else {
                    graph.add_edge(
                        src_idx,
                        dst_idx,
                        CouplingEdge {
                            failure_count: count,
                        },
                    );
                }
            }
        }

        GraphIndex { graph }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CandidateKind, FailureCategory, FailureEvent, FailureScope, MutantId, MutantStatus,
        OperatorKind, VerifierKind,
    };

    fn make_result(candidate_file: &str, external_files: &[&str]) -> MutantResult {
        let candidate = crate::model::Candidate {
            id: MutantId(format!("{candidate_file}::func::op")),
            file: Utf8PathBuf::from(candidate_file),
            symbol: "func".into(),
            kind: CandidateKind::Function,
            operator: OperatorKind::AddRequiredParameter,
            line: 1,
            byte_start: 0,
            byte_end: 0,
        };
        let external_failures = external_files
            .iter()
            .map(|f| FailureEvent {
                mutant_id: candidate.id.clone(),
                command: VerifierKind::Pytest,
                file: Utf8PathBuf::from(*f),
                line: None,
                column: None,
                symbol: None,
                category: FailureCategory::TestAssertion,
                message: String::new(),
                scope: FailureScope::External,
            })
            .collect();
        MutantResult {
            candidate,
            status: MutantStatus::Breaks,
            local_failures: vec![],
            external_failures,
        }
    }

    #[test]
    fn test_is_test_file_with_tests_directory_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/tests/helper.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_test_prefix_in_path_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/test_utils.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_underscore_test_suffix_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/order_test.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_top_level_tests_prefix_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("tests/test_order.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_source_path_should_return_false() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/domain/order.py"));
        assert_eq!(actual, FileRole::Source);
    }

    #[test]
    fn test_build_should_skip_result_with_no_external_failures() {
        let result = make_result("src/a.py", &[]);
        let idx = GraphIndex::build(&[result], &[]);
        assert_eq!(idx.graph.node_count(), 0);
    }

    #[test]
    fn test_build_should_add_edge_between_mutated_and_affected_file() {
        let result = make_result("src/a.py", &["src/b.py"]);
        let idx = GraphIndex::build(&[result], &[]);
        assert_eq!(idx.graph.edge_count(), 1);
    }

    #[test]
    fn test_build_should_accumulate_edge_weight_for_repeated_failures() {
        let r1 = make_result("src/a.py", &["src/b.py"]);
        let r2 = make_result("src/a.py", &["src/b.py"]);
        let idx = GraphIndex::build(&[r1, r2], &[]);
        let edge = idx.graph.edge_indices().next().unwrap();
        assert_eq!(idx.graph[edge].failure_count, 2);
    }

    #[test]
    fn test_resolve_source_module_with_test_prefix_should_find_matching_source() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/test_order.py"),
            &[Utf8PathBuf::from("src/order.py")],
        );
        assert_eq!(actual, Some(Utf8PathBuf::from("src/order.py")));
    }

    #[test]
    fn test_resolve_source_module_with_test_suffix_should_find_matching_source() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/order_test.py"),
            &[Utf8PathBuf::from("src/order.py")],
        );
        assert_eq!(actual, Some(Utf8PathBuf::from("src/order.py")));
    }

    #[test]
    fn test_resolve_source_module_with_no_match_should_return_none() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/test_order.py"),
            &[Utf8PathBuf::from("src/billing.py")],
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_resolve_source_module_with_source_file_should_return_none() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("src/order.py"),
            &[Utf8PathBuf::from("src/order.py")],
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_resolve_source_module_with_unrecognised_naming_should_return_none() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/order_spec.py"),
            &[Utf8PathBuf::from("src/order.py")],
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_resolve_source_module_with_multiple_candidates_prefers_closest_directory() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("src/orders/tests/test_order.py"),
            &[
                Utf8PathBuf::from("src/orders/order.py"),
                Utf8PathBuf::from("lib/order.py"),
            ],
        );
        assert_eq!(actual, Some(Utf8PathBuf::from("src/orders/order.py")));
    }

    #[test]
    fn test_build_with_test_failure_for_known_source_should_resolve_edge_to_source() {
        let result = make_result("src/billing.py", &["tests/test_order.py"]);
        let known = vec![
            Utf8PathBuf::from("src/billing.py"),
            Utf8PathBuf::from("src/order.py"),
        ];
        let idx = GraphIndex::build(&[result], &known);
        assert_eq!(idx.graph.edge_count(), 1);
        let edge = idx.graph.edge_indices().next().unwrap();
        let (_, dst) = idx.graph.edge_endpoints(edge).unwrap();
        assert_eq!(idx.graph[dst].path, Utf8PathBuf::from("src/order.py"));
    }

    #[test]
    fn test_build_with_own_test_failure_should_drop_self_edge() {
        let result = make_result("src/order.py", &["tests/test_order.py"]);
        let known = vec![Utf8PathBuf::from("src/order.py")];
        let idx = GraphIndex::build(&[result], &known);
        assert_eq!(idx.graph.edge_count(), 0);
    }

    #[test]
    fn test_build_with_test_failure_and_no_known_source_should_drop_edge() {
        let result = make_result("src/billing.py", &["tests/test_helper.py"]);
        let idx = GraphIndex::build(&[result], &[]);
        assert_eq!(idx.graph.edge_count(), 0);
    }

    #[test]
    fn test_build_with_mixed_source_and_test_failures_should_deduplicate_resolved_edge() {
        let result = make_result("src/billing.py", &["src/order.py", "tests/test_order.py"]);
        let known = vec![
            Utf8PathBuf::from("src/billing.py"),
            Utf8PathBuf::from("src/order.py"),
        ];
        let idx = GraphIndex::build(&[result], &known);
        assert_eq!(idx.graph.edge_count(), 1);
    }
}
