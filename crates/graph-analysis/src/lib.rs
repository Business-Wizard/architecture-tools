pub mod model;
mod rules;

pub use model::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

/// Analyse a slice of module dependencies for structural coupling problems.
/// Zero-config — no layer definitions required.
/// Pure — no I/O, no allocation beyond the return value.
#[must_use]
pub fn analyze(deps: &[lang_core::ModuleDep]) -> Vec<GraphViolation> {
    rules::run_all(deps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_core::ModuleDep;

    #[test]
    fn test_analyze_empty_module_deps_should_return_no_violations() {
        let actual = analyze(&[]);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_analyze_cycle_should_be_detected() {
        let deps = vec![
            ModuleDep {
                from: "a".to_string(),
                to: "b".to_string(),
            },
            ModuleDep {
                from: "b".to_string(),
                to: "a".to_string(),
            },
        ];
        let actual = analyze(&deps);
        assert_eq!(actual.len(), 1);
        assert!(matches!(actual[0].rule, GraphRuleId::CyclicDependency));
    }
}
