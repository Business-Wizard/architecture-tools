use camino::Utf8PathBuf;

use architecture_core::model::ArchitectureGraph;

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
    pub instability: Instability,
}

#[derive(Debug)]
pub struct MetricsResult {
    pub nodes: Vec<NodeMetrics>,
}

pub fn compute(graph: &ArchitectureGraph) -> MetricsResult {
    let nodes: Vec<NodeMetrics> = graph
        .modules
        .values()
        .filter(|m| !m.is_test())
        .map(|module| {
            let id = module.id();
            let fan_out = graph
                .module_edges
                .iter()
                .filter(|e| e.from == id && e.from != e.to)
                .count();
            let fan_in = graph
                .module_edges
                .iter()
                .filter(|e| e.to == id && e.from != e.to)
                .count();
            #[allow(clippy::cast_precision_loss)]
            let instability = Instability::new(if fan_in + fan_out == 0 {
                1.0
            } else {
                fan_out as f64 / (fan_in + fan_out) as f64
            });
            NodeMetrics {
                file: module.file_path().to_owned(),
                instability,
            }
        })
        .collect();

    MetricsResult { nodes }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use std::collections::{BTreeMap, BTreeSet};

    use architecture_core::model::{Module, ModuleId, QualifiedName};

    use super::*;
    use crate::graph::rules::make_graph;

    fn isolated_graph() -> ArchitectureGraph {
        let id = ModuleId(0);
        let mut modules = BTreeMap::new();
        modules.insert(
            id,
            Module::Source {
                id,
                name: QualifiedName("isolated".to_owned()),
                file_path: "src/isolated.py".into(),
                object_ids: BTreeSet::new(),
            },
        );
        ArchitectureGraph {
            modules,
            objects: BTreeMap::new(),
            dependencies: vec![],
            module_edges: vec![],
        }
    }

    #[test]
    fn test_isolated_node_should_have_instability_one() {
        let result = compute(&isolated_graph());

        let isolated = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "src/isolated.py")
            .expect("node should exist");

        assert_eq!(isolated.instability, Instability::new(1.0));
    }

    #[test]
    fn test_node_with_high_afferent_coupling_should_have_instability_zero() {
        // a imports hub, b imports hub → fan_in(hub)=2, fan_out(hub)=0 → I(hub)=0.0
        let result = compute(&make_graph(&[("a", "hub"), ("b", "hub")]));

        let hub = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "hub.py")
            .expect("node should exist");

        assert_eq!(hub.instability, Instability::new(0.0));
    }

    #[test]
    fn test_node_with_high_efferent_coupling_should_have_instability_one() {
        // consumer imports a and b → fan_in(a)=1, fan_out(a)=0 → I(a)=0.0
        let result = compute(&make_graph(&[("consumer", "a"), ("consumer", "b")]));

        let a = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "a.py")
            .expect("node should exist");

        assert_eq!(a.instability, Instability::new(0.0));
    }

    #[test]
    fn test_balanced_node_should_have_instability_half() {
        // a imports balanced; balanced imports b → fan_in=1, fan_out=1 → I=0.5
        let result = compute(&make_graph(&[("a", "balanced"), ("balanced", "b")]));

        let balanced = result
            .nodes
            .iter()
            .find(|n| n.file.as_str() == "balanced.py")
            .expect("node should exist");

        assert_eq!(balanced.instability, Instability::new(0.5));
    }
}
