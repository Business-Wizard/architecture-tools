use camino::Utf8PathBuf;
use petgraph::algo::connected_components;
use petgraph::visit::EdgeRef;

use crate::graph::coupling_graph::{FileRole, GraphIndex};

#[derive(Debug)]
pub struct CenterOfGravity {
    pub file: Utf8PathBuf,
    pub affected_source_code: usize,
    pub affected_test_code: usize,
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
            for e in &edges {
                let target = &idx.graph[e.target()];
                if target.role == FileRole::Test {
                    test_code += 1;
                } else {
                    source_code += 1;
                }
            }
            Some(CenterOfGravity {
                file: node.path.clone(),
                affected_source_code: source_code,
                affected_test_code: test_code,
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

pub fn refactor_hints(centers: &[CenterOfGravity]) -> Vec<String> {
    centers
        .iter()
        .take(5)
        .map(|c| {
            let total = c.affected_source_code + c.affected_test_code;
            format!(
                "{}: {} callers across {} source + {} test files — consider extracting an interface",
                c.file, total, c.affected_source_code, c.affected_test_code
            )
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

    #[test]
    fn test_refactor_hints_should_cap_at_five_entries() {
        let centers: Vec<CenterOfGravity> = (0..7)
            .map(|i| CenterOfGravity {
                file: Utf8PathBuf::from(format!("src/file{i}.py")),
                affected_source_code: 1,
                affected_test_code: 0,
            })
            .collect();
        let actual = refactor_hints(&centers);
        assert_eq!(actual.len(), 5);
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
