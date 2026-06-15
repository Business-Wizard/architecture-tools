use std::collections::{BTreeMap, BTreeSet};

use architecture_core::model::{ArchitectureGraph, ModuleId};

use crate::graph::violations::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

const MIN_GOD_THRESHOLD: usize = 3;

#[must_use]
pub fn check(
    graph: &ArchitectureGraph,
    dep_map: &BTreeMap<ModuleId, BTreeSet<ModuleId>>,
) -> Vec<GraphViolation> {
    if dep_map.is_empty() {
        return vec![];
    }

    let counts: Vec<usize> = dep_map.values().map(BTreeSet::len).collect();
    let threshold = derived_threshold(&counts).max(MIN_GOD_THRESHOLD);

    dep_map
        .iter()
        .filter(|(_, targets)| targets.len() >= threshold)
        .map(|(&mid, targets)| {
            let module = graph.modules[&mid].name().0.clone();
            let fan_out = targets.len();
            GraphViolation {
                rule: GraphRuleId::GodModule,
                severity: GraphSeverity::Warning,
                message: format!("{module}  (fan-out: {fan_out}, threshold: {threshold})"),
                kind: ViolationKind::GodModule {
                    module,
                    fan_out,
                    threshold,
                },
            }
        })
        .collect()
}

fn derived_threshold(counts: &[usize]) -> usize {
    let n = counts.len();
    if n == 0 {
        return MIN_GOD_THRESHOLD;
    }
    #[allow(clippy::cast_precision_loss)]
    let n_f = n as f64;
    #[allow(clippy::cast_precision_loss)]
    let mean = counts.iter().map(|&c| c as f64).sum::<f64>() / n_f;
    #[allow(clippy::cast_precision_loss)]
    let variance = counts
        .iter()
        .map(|&c| (c as f64 - mean).powi(2))
        .sum::<f64>()
        / n_f;
    let stddev = variance.sqrt();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let threshold = (mean + 2.0 * stddev).ceil() as usize;
    threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::rules::make_graph;

    #[test]
    fn test_god_module_empty_module_deps_should_produce_no_violations() {
        let graph = make_graph(&[]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_god_module_above_threshold_should_produce_violation() {
        let mut pairs: Vec<(&str, &str)> = vec![
            ("m1", "x1"),
            ("m2", "x2"),
            ("m3", "x3"),
            ("m4", "x4"),
            ("m5", "x5"),
        ];
        for tgt in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            pairs.push(("god", tgt));
        }
        let graph = make_graph(&pairs);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].rule, GraphRuleId::GodModule);
    }

    #[test]
    fn test_god_module_duplicate_edges_should_count_unique_imports_only() {
        let graph = make_graph(&[("god", "a"), ("god", "a"), ("god", "a")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_god_module_below_threshold_should_produce_no_violation() {
        let graph = make_graph(&[("god", "a")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_same_module_self_edge_should_not_count_toward_fan_out() {
        let graph = make_graph(&[("a", "a"), ("a", "b")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_multiple_edges_to_same_target_module_should_count_as_one() {
        let graph = make_graph(&[("a", "b"), ("a", "b")]);
        let dep_map = super::super::module_dep_map(&graph);
        let fan_out = dep_map[dep_map
            .keys()
            .find(|&&mid| graph.modules[&mid].name().0 == "a")
            .unwrap()]
        .len();
        assert_eq!(fan_out, 1);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }
}
