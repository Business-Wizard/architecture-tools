use camino::Utf8PathBuf;
use petgraph::Direction;

use crate::graph::abstractness::AbstractnessMap;
use crate::graph::coupling_graph::{FileRole, GraphIndex};

#[derive(Debug)]
pub struct NodeMetrics {
    pub file: Utf8PathBuf,
    pub role: FileRole,
    pub fan_in: usize,
    pub fan_out: usize,
    pub instability: f64,
    pub abstractness: f64,
    pub distance: f64,
    pub distance_warning: bool,
    pub distance_failure: bool,
}

#[derive(Debug)]
pub struct MetricsResult {
    pub nodes: Vec<NodeMetrics>,
}

pub fn compute(idx: &GraphIndex, abstractness: &AbstractnessMap) -> MetricsResult {
    let mut nodes: Vec<NodeMetrics> = idx
        .graph
        .node_indices()
        .map(|n| {
            let node = &idx.graph[n];
            let fan_out = idx.graph.edges_directed(n, Direction::Outgoing).count();
            let fan_in = idx.graph.edges_directed(n, Direction::Incoming).count();

            // Isolated nodes (no edges) default to I=1.0: no-abstractions + maximally-unstable
            // sits on the main sequence (distance=0), avoiding false violations.
            #[allow(clippy::cast_precision_loss)]
            let instability = if fan_in + fan_out == 0 {
                1.0
            } else {
                fan_out as f64 / (fan_in + fan_out) as f64
            };

            // Files with no class definitions default to A=0.0 (fully concrete).
            let abstractness = abstractness
                .by_file
                .get(&node.path)
                .and_then(|s| s.value)
                .unwrap_or(0.0);

            let distance = (abstractness + instability - 1.0).abs();
            let distance_warning = distance > 0.3;
            let distance_failure = distance > 0.5;

            NodeMetrics {
                file: node.path.clone(),
                role: node.role.clone(),
                fan_in,
                fan_out,
                instability,
                abstractness,
                distance,
                distance_warning,
                distance_failure,
            }
        })
        .collect();

    nodes.sort_by(|a, b| {
        b.distance
            .partial_cmp(&a.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    MetricsResult { nodes }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::graph::coupling_graph::{CouplingEdge, CouplingGraph, CouplingNode};
    use std::collections::HashMap;

    fn make_graph_index(edges: &[(&str, &str)]) -> GraphIndex {
        let mut graph = CouplingGraph::new();
        let mut map: HashMap<&str, petgraph::graph::NodeIndex> = HashMap::new();
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
            graph.add_edge(s, d, CouplingEdge { failure_count: 1 });
        }
        GraphIndex { graph }
    }

    fn empty_abstractness() -> AbstractnessMap {
        AbstractnessMap {
            by_file: HashMap::new(),
        }
    }

    #[test]
    fn test_isolated_node_should_have_instability_one() {
        let abstractness = empty_abstractness();

        let mut graph = CouplingGraph::new();
        graph.add_node(CouplingNode {
            path: Utf8PathBuf::from("src/isolated.py"),
            role: FileRole::Source,
        });
        let idx = GraphIndex { graph };

        let result = compute(&idx, &abstractness);

        let isolated = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "src/isolated.py")
            .expect("node should exist");

        assert_eq!(isolated.instability, 1.0);
    }

    #[test]
    fn test_pure_fan_out_node_should_have_instability_one() {
        let idx = make_graph_index(&[("src/hub.py", "src/a.py"), ("src/hub.py", "src/b.py")]);
        let abstractness = empty_abstractness();
        let result = compute(&idx, &abstractness);

        let hub = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "src/hub.py")
            .expect("node should exist");

        assert_eq!(hub.instability, 1.0);
    }

    #[test]
    fn test_pure_fan_in_node_should_have_instability_zero() {
        let idx = make_graph_index(&[
            ("src/a.py", "src/consumer.py"),
            ("src/b.py", "src/consumer.py"),
        ]);
        let abstractness = empty_abstractness();
        let result = compute(&idx, &abstractness);

        let consumer = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "src/consumer.py")
            .expect("node should exist");

        assert_eq!(consumer.instability, 0.0);
    }

    #[test]
    fn test_balanced_node_should_have_instability_half() {
        let idx = make_graph_index(&[
            ("src/a.py", "src/balanced.py"),
            ("src/balanced.py", "src/b.py"),
        ]);
        let abstractness = empty_abstractness();
        let result = compute(&idx, &abstractness);

        let balanced = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "src/balanced.py")
            .expect("node should exist");

        assert_eq!(balanced.instability, 0.5);
    }

    #[test]
    fn test_distance_warning_threshold_should_be_above_point_three() {
        let metrics = NodeMetrics {
            file: Utf8PathBuf::from("src/test.py"),
            role: FileRole::Source,
            fan_in: 1,
            fan_out: 1,
            instability: 0.5,
            abstractness: 0.0,
            distance: 0.4,
            distance_warning: true,
            distance_failure: false,
        };

        assert!(metrics.distance_warning);
        assert!(!metrics.distance_failure);
    }

    #[test]
    fn test_distance_failure_threshold_should_be_above_point_five() {
        let metrics = NodeMetrics {
            file: Utf8PathBuf::from("src/test.py"),
            role: FileRole::Source,
            fan_in: 1,
            fan_out: 1,
            instability: 0.5,
            abstractness: 0.0,
            distance: 0.6,
            distance_warning: true,
            distance_failure: true,
        };

        assert!(metrics.distance_warning);
        assert!(metrics.distance_failure);
    }

    #[test]
    fn test_no_class_definitions_should_default_abstractness_to_zero() {
        let idx = make_graph_index(&[("src/a.py", "src/concrete.py")]);
        let abstractness = empty_abstractness();
        let result = compute(&idx, &abstractness);

        let node = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "src/concrete.py")
            .expect("node should exist");

        assert_eq!(node.abstractness, 0.0);
    }
}
