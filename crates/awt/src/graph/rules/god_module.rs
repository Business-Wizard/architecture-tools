use std::collections::{HashMap, HashSet};

use lang_core::ModuleDep;

use crate::graph::violations::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

const MIN_GOD_THRESHOLD: usize = 3;

#[must_use]
pub fn check(deps: &[ModuleDep]) -> Vec<GraphViolation> {
    let mut fan_out: HashMap<&str, HashSet<&str>> = HashMap::new();
    for dep in deps {
        fan_out
            .entry(dep.from.as_str())
            .or_default()
            .insert(dep.to.as_str());
    }

    if fan_out.is_empty() {
        return vec![];
    }

    let counts: Vec<usize> = fan_out.values().map(HashSet::len).collect();
    let threshold = derived_threshold(&counts).max(MIN_GOD_THRESHOLD);

    fan_out
        .into_iter()
        .filter(|(_, imports)| imports.len() >= threshold)
        .map(|(module, imports)| {
            let fan_out = imports.len();
            GraphViolation {
                rule: GraphRuleId::GodModule,
                severity: GraphSeverity::Warning,
                message: format!("{module}  (fan-out: {fan_out}, threshold: {threshold})"),
                kind: ViolationKind::GodModule {
                    module: module.to_string(),
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

    fn make_deps(pairs: &[(&str, &str)]) -> Vec<ModuleDep> {
        pairs
            .iter()
            .map(|(f, t)| ModuleDep {
                from: (*f).into(),
                to: (*t).into(),
            })
            .collect()
    }

    #[test]
    fn test_god_module_empty_module_deps_should_produce_no_violations() {
        let actual = check(&make_deps(&[]));
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_god_module_above_threshold_should_produce_violation() {
        let mut deps: Vec<(&str, &str)> = vec![
            ("m1", "x1"),
            ("m2", "x2"),
            ("m3", "x3"),
            ("m4", "x4"),
            ("m5", "x5"),
        ];
        for tgt in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            deps.push(("god", tgt));
        }
        let actual = check(&make_deps(&deps));
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].rule, GraphRuleId::GodModule);
    }

    #[test]
    fn test_god_module_duplicate_edges_should_count_unique_imports_only() {
        let actual = check(&make_deps(&[("god", "a"), ("god", "a"), ("god", "a")]));
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_god_module_below_threshold_should_produce_no_violation() {
        let actual = check(&make_deps(&[("god", "a")]));
        assert_eq!(actual, vec![]);
    }
}
