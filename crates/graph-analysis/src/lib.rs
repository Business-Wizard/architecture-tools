pub mod config;
mod layer_resolver;
pub mod model;
mod rules;

pub use config::{GraphLayerConfig, LayerDef};
pub use model::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

use layer_resolver::LayerResolver;

/// Analyse an `InspectResult` for layer violations and coupling smells.
/// Pure — no I/O, no allocation beyond the return value.
#[must_use]
pub fn analyze(
    result: &py_analyzer::InspectResult,
    config: &GraphLayerConfig,
) -> Vec<GraphViolation> {
    let resolver = LayerResolver::new(config);
    rules::run_all(result, &resolver, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use py_analyzer::{ClassDef, InspectResult};

    fn empty_result() -> InspectResult {
        InspectResult {
            module_deps: vec![],
            classes: vec![],
        }
    }

    #[test]
    fn test_analyze_empty_result_should_return_no_violations() {
        let actual = analyze(&empty_result(), &GraphLayerConfig::default());
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_analyze_with_empty_layer_config_should_still_run_fan_in() {
        // 9 classes all referencing one target — fan-in rule fires even without layer config.
        let classes: Vec<ClassDef> = (0..9)
            .map(|i| ClassDef {
                module: "mod".to_string(),
                name: format!("User{i}"),
                bases: vec![],
                attributes: vec![],
                methods: vec![],
                class_deps: vec!["domain.Error".to_string()],
            })
            .collect();
        let result = InspectResult {
            module_deps: vec![],
            classes,
        };
        let config = GraphLayerConfig {
            layers: vec![],
            fan_in_threshold: 8,
        };
        let actual = analyze(&result, &config);
        assert_eq!(actual.len(), 1);
        assert!(matches!(actual[0].rule, GraphRuleId::HighFanIn));
    }
}
