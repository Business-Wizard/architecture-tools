use std::collections::HashMap;

use camino::Utf8PathBuf;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::model::MutantResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRole {
    Source,
    Test,
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

impl GraphIndex {
    pub fn build(results: &[MutantResult]) -> Self {
        let mut graph = CouplingGraph::new();
        let mut node_map: HashMap<Utf8PathBuf, NodeIndex> = HashMap::new();

        let get_or_insert = |g: &mut CouplingGraph,
                             map: &mut HashMap<Utf8PathBuf, NodeIndex>,
                             path: Utf8PathBuf| {
            if let Some(&idx) = map.get(&path) {
                return idx;
            }
            let role = if is_test_file(&path) {
                FileRole::Test
            } else {
                FileRole::Source
            };
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
                let dst_idx = get_or_insert(&mut graph, &mut node_map, affected_file);
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

fn is_test_file(path: &Utf8PathBuf) -> bool {
    let s = path.as_str();
    s.contains("/tests/")
        || s.contains("/test_")
        || s.ends_with("_test.py")
        || s.starts_with("tests/")
        || s.starts_with("test_")
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
        let actual = is_test_file(&Utf8PathBuf::from("src/tests/helper.py"));
        assert!(actual);
    }

    #[test]
    fn test_is_test_file_with_test_prefix_in_path_should_return_true() {
        let actual = is_test_file(&Utf8PathBuf::from("src/test_utils.py"));
        assert!(actual);
    }

    #[test]
    fn test_is_test_file_with_underscore_test_suffix_should_return_true() {
        let actual = is_test_file(&Utf8PathBuf::from("src/order_test.py"));
        assert!(actual);
    }

    #[test]
    fn test_is_test_file_with_top_level_tests_prefix_should_return_true() {
        let actual = is_test_file(&Utf8PathBuf::from("tests/test_order.py"));
        assert!(actual);
    }

    #[test]
    fn test_is_test_file_with_source_path_should_return_false() {
        let actual = is_test_file(&Utf8PathBuf::from("src/domain/order.py"));
        assert!(!actual);
    }

    #[test]
    fn test_build_should_skip_result_with_no_external_failures() {
        let result = make_result("src/a.py", &[]);
        let idx = GraphIndex::build(&[result]);
        assert_eq!(idx.graph.node_count(), 0);
    }

    #[test]
    fn test_build_should_add_edge_between_mutated_and_affected_file() {
        let result = make_result("src/a.py", &["src/b.py"]);
        let idx = GraphIndex::build(&[result]);
        assert_eq!(idx.graph.edge_count(), 1);
    }

    #[test]
    fn test_build_should_accumulate_edge_weight_for_repeated_failures() {
        let r1 = make_result("src/a.py", &["src/b.py"]);
        let r2 = make_result("src/a.py", &["src/b.py"]);
        let idx = GraphIndex::build(&[r1, r2]);
        let edge = idx.graph.edge_indices().next().unwrap();
        assert_eq!(idx.graph[edge].failure_count, 2);
    }
}
