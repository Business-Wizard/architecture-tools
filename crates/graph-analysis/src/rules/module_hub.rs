use std::collections::{HashMap, HashSet};

use lang_core::ModuleDep;

use crate::model::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

const MIN_HUB_THRESHOLD: usize = 3;

#[must_use]
pub fn check(deps: &[ModuleDep]) -> Vec<GraphViolation> {
    // Count unique importers per module (deduplicate (from, to) pairs).
    let mut fan_in: HashMap<&str, HashSet<&str>> = HashMap::new();
    for dep in deps {
        fan_in
            .entry(dep.to.as_str())
            .or_default()
            .insert(dep.from.as_str());
    }

    if fan_in.is_empty() {
        return vec![];
    }

    let counts: Vec<usize> = fan_in.values().map(HashSet::len).collect();
    let threshold = derived_threshold(&counts).max(MIN_HUB_THRESHOLD);

    fan_in
        .into_iter()
        .filter(|(_, importers)| importers.len() >= threshold)
        .map(|(module, importers)| {
            let fan_in = importers.len();
            GraphViolation {
                rule: GraphRuleId::ModuleHub,
                severity: GraphSeverity::Warning,
                message: format!("{module}  (fan-in: {fan_in}, threshold: {threshold})"),
                kind: ViolationKind::ModuleHub {
                    module: module.to_string(),
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

    fn make_deps(pairs: &[(&str, &str)]) -> Vec<ModuleDep> {
        pairs
            .iter()
            .map(|(f, t)| ModuleDep {
                from: (*f).to_string(),
                to: (*t).to_string(),
            })
            .collect()
    }

    #[test]
    fn test_hub_empty_module_deps_should_produce_no_violations() {
        let actual = check(&make_deps(&[]));
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_hub_above_threshold_should_produce_violation() {
        // Most modules imported by 1 other; "hub" imported by 10 — clear outlier above mean+2σ.
        let mut deps: Vec<(&str, &str)> = vec![
            ("x1", "m1"),
            ("x2", "m2"),
            ("x3", "m3"),
            ("x4", "m4"),
            ("x5", "m5"),
        ];
        for src in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            deps.push((src, "hub"));
        }
        let actual = check(&make_deps(&deps));
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].rule, GraphRuleId::ModuleHub);
    }

    #[test]
    fn test_hub_duplicate_edges_should_count_unique_importers_only() {
        // Same importer repeated — should count as 1.
        let actual = check(&make_deps(&[("a", "hub"), ("a", "hub"), ("a", "hub")]));
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_hub_below_threshold_should_produce_no_violation() {
        // Only 1 importer — well below any threshold.
        let actual = check(&make_deps(&[("a", "hub")]));
        assert_eq!(actual, vec![]);
    }
}
