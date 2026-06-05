use camino::Utf8PathBuf;
use petgraph::Direction;

use crate::graph::abstractness::AbstractnessMap;
use crate::graph::coupling_graph::{FileRole, GraphIndex};

pub const INSTABILITY_EPSILON: f64 = 0.01;

/// Instability I ∈ [0,1]: 0 = maximally stable, 1 = maximally unstable.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Instability(f64);

impl Instability {
    pub fn new(value: f64) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    pub fn as_f64(self) -> f64 {
        self.0
    }
}

/// Role newtype: the module that depends on something (coupling-graph dst node).
#[derive(Debug, Clone, Copy)]
pub struct Depender(pub Instability);

/// Role newtype: the module that is depended upon (coupling-graph src node).
#[derive(Debug, Clone, Copy)]
pub struct Dependency(pub Instability);

/// Returns true when the dependency is more unstable than the depender — an SDP violation.
/// SDP: you should only depend on things more stable (lower I) than yourself.
/// Violation: dependency.I > depender.I + ε (depending on something more unstable than you).
pub fn violates_sdp(dependency: Dependency, depender: Depender) -> bool {
    dependency.0.as_f64() > depender.0.as_f64() + INSTABILITY_EPSILON
}

#[derive(Debug)]
pub struct NodeMetrics {
    pub file: Utf8PathBuf,
    pub role: FileRole,
    pub instability: Instability,
    pub abstractness: f64,
    pub distance: f64,
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
            // Coupling graph edge A→B means "mutating A broke B", i.e. B depends on A.
            // Outgoing edges from A = things that depend on A = A's afferent coupling (fan-in).
            // Incoming edges to A = things A depends on = A's efferent coupling (fan-out).
            let fan_in = idx.graph.edges_directed(n, Direction::Outgoing).count();
            let fan_out = idx.graph.edges_directed(n, Direction::Incoming).count();

            // Isolated nodes (no edges) default to I=1.0: no-abstractions + maximally-unstable
            // sits on the main sequence (distance=0), avoiding false violations.
            #[allow(clippy::cast_precision_loss)]
            let instability = Instability::new(if fan_in + fan_out == 0 {
                1.0
            } else {
                fan_out as f64 / (fan_in + fan_out) as f64
            });

            // Files with no class definitions default to A=0.0 (fully concrete).
            let abstractness = abstractness
                .by_file
                .get(&node.path)
                .and_then(|s| s.value)
                .unwrap_or(0.0);

            let distance = (abstractness + instability.as_f64() - 1.0).abs();

            NodeMetrics {
                file: node.path.clone(),
                role: node.role.clone(),
                instability,
                abstractness,
                distance,
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

        assert_eq!(isolated.instability, Instability::new(1.0));
    }

    #[test]
    fn test_node_broken_by_many_mutations_should_have_instability_one() {
        // hub→a and hub→b: mutating hub breaks a and b, so a and b depend on hub.
        // hub has high afferent coupling (many dependents) → I=0 (stable).
        // a and b have no dependents and depend on hub → I=1 (unstable).
        let idx = make_graph_index(&[("src/hub.py", "src/a.py"), ("src/hub.py", "src/b.py")]);
        let abstractness = empty_abstractness();
        let result = compute(&idx, &abstractness);

        let a = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "src/a.py")
            .expect("node should exist");

        assert_eq!(a.instability, Instability::new(1.0));
    }

    #[test]
    fn test_node_whose_mutations_break_many_should_have_instability_zero() {
        // a→consumer and b→consumer: mutating a or b breaks consumer, so consumer depends on both.
        // consumer has no outgoing coupling edges → nothing depends on consumer → I=1 (unstable).
        // a and b each break consumer → consumer depends on them → a and b are stable (I=0).
        let idx = make_graph_index(&[
            ("src/a.py", "src/consumer.py"),
            ("src/b.py", "src/consumer.py"),
        ]);
        let abstractness = empty_abstractness();
        let result = compute(&idx, &abstractness);

        let a = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "src/a.py")
            .expect("node should exist");

        assert_eq!(a.instability, Instability::new(0.0));
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

        assert_eq!(balanced.instability, Instability::new(0.5));
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
