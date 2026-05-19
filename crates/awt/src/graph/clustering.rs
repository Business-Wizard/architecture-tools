use camino::Utf8PathBuf;
use petgraph::algo::connected_components;
use petgraph::visit::EdgeRef;

use crate::graph::coupling_graph::{FileRole, GraphIndex};

#[derive(Debug)]
pub struct CenterOfGravity {
    pub file: Utf8PathBuf,
    pub affected_source_code: usize,
    pub affected_test_code: usize,
    pub edge_count: usize,
    pub total_failure_count: usize,
    pub heaviest_neighbor: Option<Utf8PathBuf>,
}

impl CenterOfGravity {
    fn heaviest_neighbor_is_own_test(&self) -> bool {
        let Some(stem) = self.file.file_stem() else {
            return false;
        };
        self.heaviest_neighbor.as_ref().is_some_and(|n| {
            n.file_name().is_some_and(|name| {
                name == format!("test_{stem}.py") || name == format!("{stem}_test.py")
            })
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum RefactorHint {
    ExtractTestFixture,
    BrittleCoupling { neighbor: String },
    StabilizeApiSurface,
    NoRecommendation,
}

#[derive(Debug)]
pub struct UnexpectedCoupling {
    pub mutant_file: Utf8PathBuf,
    pub affected_file: Utf8PathBuf,
    pub failure_count: usize,
}

#[derive(Debug)]
pub struct ClusteringResult {
    pub centers: Vec<CenterOfGravity>,
    pub unexpected: Vec<UnexpectedCoupling>,
    pub component_count: usize,
}

pub fn analyse(idx: &GraphIndex) -> ClusteringResult {
    let component_count = connected_components(&idx.graph);

    // Centers of gravity: nodes with highest out-degree (by distinct affected files)
    let mut centers: Vec<CenterOfGravity> = idx
        .graph
        .node_indices()
        .filter_map(|n| {
            let node = &idx.graph[n];
            let edges: Vec<_> = idx.graph.edges(n).collect();
            if edges.is_empty() {
                return None;
            }
            let mut source_code = 0usize;
            let mut test_code = 0usize;
            let mut total_failure_count = 0usize;
            let mut heaviest_neighbor: Option<(usize, Utf8PathBuf)> = None;
            for e in &edges {
                let target = &idx.graph[e.target()];
                let failures = e.weight().failure_count;
                if target.role == FileRole::Test {
                    test_code += 1;
                } else {
                    source_code += 1;
                }
                total_failure_count += failures;
                if heaviest_neighbor
                    .as_ref()
                    .is_none_or(|(max, _)| failures > *max)
                {
                    heaviest_neighbor = Some((failures, target.path.clone()));
                }
            }
            Some(CenterOfGravity {
                file: node.path.clone(),
                affected_source_code: source_code,
                affected_test_code: test_code,
                edge_count: edges.len(),
                total_failure_count,
                heaviest_neighbor: heaviest_neighbor.map(|(_, p)| p),
            })
        })
        .collect();

    centers.sort_by(|a, b| {
        (b.affected_source_code + b.affected_test_code)
            .cmp(&(a.affected_source_code + a.affected_test_code))
    });

    // Unexpected coupling: edge where source and target have different top-level packages
    let mut unexpected: Vec<UnexpectedCoupling> = idx
        .graph
        .edge_indices()
        .filter_map(|e| {
            let (src, dst) = idx.graph.edge_endpoints(e)?;
            let src_pkg = top_package(&idx.graph[src].path);
            let dst_pkg = top_package(&idx.graph[dst].path);
            if src_pkg == dst_pkg {
                return None;
            }
            Some(UnexpectedCoupling {
                mutant_file: idx.graph[src].path.clone(),
                affected_file: idx.graph[dst].path.clone(),
                failure_count: idx.graph[e].failure_count,
            })
        })
        .collect();

    unexpected.sort_by_key(|b| std::cmp::Reverse(b.failure_count));

    ClusteringResult {
        centers,
        unexpected,
        component_count,
    }
}

fn top_package(path: &Utf8PathBuf) -> String {
    path.components()
        .next()
        .map(|c| c.as_str().to_string())
        .unwrap_or_default()
}

const BRITTLE_AVG_THRESHOLD: usize = 4;
const BROAD_EDGE_THRESHOLD: usize = 6;

pub fn refactor_hints(centers: &[CenterOfGravity]) -> Vec<(Utf8PathBuf, RefactorHint)> {
    centers
        .iter()
        .take(5)
        .map(|c| {
            let hint = if c.affected_source_code == 0 && c.affected_test_code > 0 {
                RefactorHint::ExtractTestFixture
            } else if c.edge_count > 0
                && c.edge_count <= 3
                && c.total_failure_count / c.edge_count >= BRITTLE_AVG_THRESHOLD
            {
                let neighbor = c
                    .heaviest_neighbor
                    .as_ref()
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_default();
                if c.heaviest_neighbor_is_own_test() {
                    RefactorHint::NoRecommendation
                } else {
                    RefactorHint::BrittleCoupling { neighbor }
                }
            } else if c.edge_count >= BROAD_EDGE_THRESHOLD {
                RefactorHint::StabilizeApiSurface
            } else {
                RefactorHint::NoRecommendation
            };
            (c.file.clone(), hint)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::coupling_graph::{CouplingEdge, CouplingGraph, CouplingNode, FileRole};

    fn make_graph_index(edges: &[(&str, &str)]) -> GraphIndex {
        let mut graph = CouplingGraph::new();
        let mut map: std::collections::HashMap<&str, petgraph::graph::NodeIndex> =
            std::collections::HashMap::new();
        for (src, dst) in edges {
            let s = *map.entry(src).or_insert_with(|| {
                graph.add_node(CouplingNode {
                    path: Utf8PathBuf::from(*src),
                    role: FileRole::Source,
                })
            });
            let d = *map.entry(dst).or_insert_with(|| {
                graph.add_node(CouplingNode {
                    path: Utf8PathBuf::from(*dst),
                    role: FileRole::Source,
                })
            });
            graph.add_edge(s, d, CouplingEdge { failure_count: 3 });
        }
        GraphIndex { graph }
    }

    #[test]
    fn test_top_package_with_nested_path_should_return_first_component() {
        let actual = top_package(&Utf8PathBuf::from("src/domain/order.py"));
        assert_eq!(actual, "src");
    }

    #[test]
    fn test_top_package_with_single_component_should_return_it() {
        let actual = top_package(&Utf8PathBuf::from("order.py"));
        assert_eq!(actual, "order.py");
    }

    #[test]
    fn test_top_package_with_empty_path_should_return_empty_string() {
        let actual = top_package(&Utf8PathBuf::from(""));
        assert_eq!(actual, "");
    }

    fn make_center(
        file: &str,
        source: usize,
        test: usize,
        edge_count: usize,
        total_failures: usize,
        heaviest: Option<&str>,
    ) -> CenterOfGravity {
        CenterOfGravity {
            file: Utf8PathBuf::from(file),
            affected_source_code: source,
            affected_test_code: test,
            edge_count,
            total_failure_count: total_failures,
            heaviest_neighbor: heaviest.map(Utf8PathBuf::from),
        }
    }

    #[test]
    fn test_refactor_hints_should_cap_at_five_entries() {
        let centers: Vec<CenterOfGravity> = (0..7)
            .map(|i| make_center(&format!("src/file{i}.py"), 1, 0, 1, 1, None))
            .collect();
        let actual = refactor_hints(&centers);
        assert_eq!(actual.len(), 5);
    }

    #[test]
    fn test_refactor_hints_with_test_only_coupling_should_suggest_extract_fixture() {
        let centers = vec![make_center("src/a.py", 0, 3, 3, 9, None)];
        let actual = refactor_hints(&centers);
        assert_eq!(
            actual,
            vec![(
                Utf8PathBuf::from("src/a.py"),
                RefactorHint::ExtractTestFixture
            )]
        );
    }

    #[test]
    fn test_refactor_hints_with_high_avg_failures_should_suggest_brittle_coupling() {
        let centers = vec![make_center("src/a.py", 2, 0, 2, 10, Some("src/b.py"))];
        let actual = refactor_hints(&centers);
        assert_eq!(
            actual,
            vec![(
                Utf8PathBuf::from("src/a.py"),
                RefactorHint::BrittleCoupling {
                    neighbor: "src/b.py".to_string()
                }
            )]
        );
    }

    #[test]
    fn test_refactor_hints_with_many_edges_should_suggest_stabilize_api() {
        let centers = vec![make_center("src/a.py", 4, 2, 8, 8, None)];
        let actual = refactor_hints(&centers);
        assert_eq!(
            actual,
            vec![(
                Utf8PathBuf::from("src/a.py"),
                RefactorHint::StabilizeApiSurface
            )]
        );
    }

    #[test]
    fn test_refactor_hints_with_no_clear_signal_should_return_no_recommendation() {
        let centers = vec![make_center("src/a.py", 2, 1, 3, 3, None)];
        let actual = refactor_hints(&centers);
        assert_eq!(
            actual,
            vec![(
                Utf8PathBuf::from("src/a.py"),
                RefactorHint::NoRecommendation
            )]
        );
    }

    #[test]
    fn test_heaviest_neighbor_is_own_test_with_test_prefix_should_return_true() {
        let center = make_center("src/order.py", 0, 1, 1, 1, Some("tests/test_order.py"));
        assert!(center.heaviest_neighbor_is_own_test());
    }

    #[test]
    fn test_heaviest_neighbor_is_own_test_with_test_suffix_should_return_true() {
        let center = make_center("src/order.py", 0, 1, 1, 1, Some("tests/order_test.py"));
        assert!(center.heaviest_neighbor_is_own_test());
    }

    #[test]
    fn test_heaviest_neighbor_is_own_test_with_unrelated_test_should_return_false() {
        let center = make_center("src/order.py", 0, 1, 1, 1, Some("tests/test_invoice.py"));
        assert!(!center.heaviest_neighbor_is_own_test());
    }

    #[test]
    fn test_refactor_hints_with_brittle_coupling_to_own_test_should_return_no_recommendation() {
        let centers = vec![make_center(
            "src/order.py",
            2,
            0,
            2,
            10,
            Some("tests/test_order.py"),
        )];
        let actual = refactor_hints(&centers);
        assert_eq!(
            actual,
            vec![(
                Utf8PathBuf::from("src/order.py"),
                RefactorHint::NoRecommendation
            )]
        );
    }

    #[test]
    fn test_analyse_should_flag_different_package_edge_as_unexpected() {
        let idx = make_graph_index(&[("src/a/order.py", "lib/b/report.py")]);
        let result = analyse(&idx);
        assert_eq!(result.unexpected.len(), 1);
    }

    #[test]
    fn test_analyse_should_not_flag_same_package_edge_as_unexpected() {
        let idx = make_graph_index(&[("src/a/order.py", "src/b/invoice.py")]);
        let result = analyse(&idx);
        assert!(result.unexpected.is_empty());
    }
}
