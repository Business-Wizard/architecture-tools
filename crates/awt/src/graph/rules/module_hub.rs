use std::collections::{BTreeMap, BTreeSet};

use architecture_core::model::{ArchitectureGraph, ModuleId};

use crate::graph::violations::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

const MIN_HUB_THRESHOLD: usize = 3;

#[must_use]
pub fn check(
    graph: &ArchitectureGraph,
    dep_map: &BTreeMap<ModuleId, BTreeSet<ModuleId>>,
) -> Vec<GraphViolation> {
    let mut fan_in: BTreeMap<ModuleId, BTreeSet<ModuleId>> = BTreeMap::new();
    for &mid in graph.modules.keys() {
        fan_in.entry(mid).or_default();
    }
    for (&src, targets) in dep_map {
        for &target in targets {
            fan_in.entry(target).or_default().insert(src);
        }
    }

    if fan_in.is_empty() {
        return vec![];
    }

    let counts: Vec<usize> = fan_in.values().map(BTreeSet::len).collect();
    let threshold = derived_threshold(&counts).max(MIN_HUB_THRESHOLD);

    fan_in
        .iter()
        .filter(|(_, importers)| importers.len() >= threshold)
        .map(|(&mid, importers)| {
            let module = graph.modules[&mid].name().0.clone();
            let fan_in = importers.len();
            GraphViolation {
                rule: GraphRuleId::ModuleHub,
                severity: GraphSeverity::Warning,
                message: format!("{module}  (fan-in: {fan_in}, threshold: {threshold})"),
                kind: ViolationKind::ModuleHub {
                    module,
                    fan_in,
                    threshold,
                },
            }
        })
        .collect()
}

fn derived_threshold(counts: &[usize]) -> usize {
    let n = counts.len();
    if n == 0 {
        return MIN_HUB_THRESHOLD;
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
    fn test_hub_empty_module_deps_should_produce_no_violations() {
        let graph = make_graph(&[]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_hub_above_threshold_should_produce_violation() {
        let mut pairs: Vec<(&str, &str)> = vec![
            ("x1", "m1"),
            ("x2", "m2"),
            ("x3", "m3"),
            ("x4", "m4"),
            ("x5", "m5"),
        ];
        for src in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            pairs.push((src, "hub"));
        }
        let graph = make_graph(&pairs);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].rule, GraphRuleId::ModuleHub);
    }

    #[test]
    fn test_hub_duplicate_edges_should_count_unique_importers_only() {
        let graph = make_graph(&[("a", "hub"), ("a", "hub"), ("a", "hub")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_hub_below_threshold_should_produce_no_violation() {
        let graph = make_graph(&[("a", "hub")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_same_module_self_edge_should_not_count_toward_fan_in() {
        let graph = make_graph(&[("a", "a"), ("b", "a")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_multiple_edges_from_same_source_module_should_count_as_one() {
        let graph = make_graph(&[("a", "hub"), ("a", "hub")]);
        let dep_map = super::super::module_dep_map(&graph);
        let hub_id = *dep_map
            .keys()
            .find(|&&mid| graph.modules[&mid].name().0 == "hub")
            .unwrap();
        let mut fan_in_for_hub: BTreeSet<ModuleId> = BTreeSet::new();
        for (&src, targets) in &dep_map {
            if targets.contains(&hub_id) {
                fan_in_for_hub.insert(src);
            }
        }
        assert_eq!(fan_in_for_hub.len(), 1);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }
}
