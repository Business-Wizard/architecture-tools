pub mod model;
mod rules;

pub use model::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

/// Analyse an `InspectResult` for structural coupling problems.
/// Zero-config — no layer definitions required.
/// Pure — no I/O, no allocation beyond the return value.
#[must_use]
pub fn analyze(result: &py_analyzer::InspectResult) -> Vec<GraphViolation> {
    rules::run_all(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use py_analyzer::{InspectResult, ModuleDep};

    fn empty_result() -> InspectResult {
        InspectResult {
            module_deps: vec![],
            classes: vec![],
        }
    }

    #[test]
    fn test_analyze_empty_module_deps_should_return_no_violations() {
        let actual = analyze(&empty_result());
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_analyze_cycle_should_be_detected() {
        let result = InspectResult {
            module_deps: vec![
                ModuleDep {
                    from: "a".to_string(),
                    to: "b".to_string(),
                },
                ModuleDep {
                    from: "b".to_string(),
                    to: "a".to_string(),
                },
            ],
            classes: vec![],
        };
        let actual = analyze(&result);
        assert_eq!(actual.len(), 1);
        assert!(matches!(actual[0].rule, GraphRuleId::CyclicDependency));
    }
}
