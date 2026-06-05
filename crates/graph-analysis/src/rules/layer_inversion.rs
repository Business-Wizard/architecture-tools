use std::collections::HashMap;

use py_analyzer::InspectResult;

use crate::layer_resolver::LayerResolver;
use crate::model::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

pub fn check(result: &InspectResult, resolver: &LayerResolver<'_>) -> Vec<GraphViolation> {
    // Build qualified class name → module map for dep resolution.
    let class_module: HashMap<String, &str> = result
        .classes
        .iter()
        .map(|c| (format!("{}.{}", c.module, c.name), c.module.as_str()))
        .collect();

    let mut violations = vec![];

    for cls in &result.classes {
        let Some((src_rank, src_layer)) = resolver.resolve(&cls.module) else {
            continue;
        };

        for dep in &cls.class_deps {
            // Extract module from qualified dep ("module.ClassName" → "module").
            let dep_module = if let Some(module) = class_module.get(dep.as_str()) {
                *module
            } else {
                // Fall back: everything before the last '.' component.
                match dep.rfind('.') {
                    Some(pos) => &dep[..pos],
                    None => continue,
                }
            };

            let Some((dep_rank, dep_layer)) = resolver.resolve(dep_module) else {
                continue;
            };

            // Inversion: a more-stable (lower-rank) layer depends on a less-stable (higher-rank) one.
            if dep_rank <= src_rank {
                continue;
            }

            let dep_class_name = dep.split('.').next_back().unwrap_or(dep.as_str());
            let fix_hint = if dep_class_name.contains("Error")
                || dep_class_name.contains("Exception")
            {
                format!("move `{dep_class_name}` to a more-stable layer (e.g. domain)")
            } else {
                format!(
                    "`{dep_class_name}` is defined in `{dep_layer}` but used by `{src_layer}`; consider moving it to a shared stable layer"
                )
            };

            violations.push(GraphViolation {
                rule: GraphRuleId::LayerInversion,
                severity: GraphSeverity::Error,
                message: format!("{}.{} -> {dep}", cls.module, cls.name),
                kind: ViolationKind::LayerInversion {
                    from_module: cls.module.clone(),
                    to_class: dep.clone(),
                    from_layer: src_layer.to_string(),
                    to_layer: dep_layer.to_string(),
                    fix_hint,
                },
            });
        }
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GraphLayerConfig, LayerDef};
    use py_analyzer::{ClassDef, InspectResult};

    fn make_config(layers: &[(&str, &[&str])]) -> GraphLayerConfig {
        GraphLayerConfig {
            layers: layers
                .iter()
                .map(|(name, prefixes)| LayerDef {
                    name: (*name).to_string(),
                    module_prefixes: prefixes.iter().map(|s| (*s).to_string()).collect(),
                })
                .collect(),
            fan_in_threshold: 8,
        }
    }

    fn make_class(module: &str, name: &str, deps: &[&str]) -> ClassDef {
        ClassDef {
            module: module.to_string(),
            name: name.to_string(),
            bases: vec![],
            attributes: vec![],
            methods: vec![],
            class_deps: deps.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    fn make_result(classes: Vec<ClassDef>) -> InspectResult {
        InspectResult {
            module_deps: vec![],
            classes,
        }
    }

    #[test]
    fn test_layer_inversion_inner_to_outer_dep_should_produce_violation() {
        // domain (rank 0) depends on usecase (rank 1) — inversion
        let config = make_config(&[("domain", &["domain"]), ("usecase", &["usecase"])]);
        let result = make_result(vec![
            make_class("domain", "OrderEntity", &["usecase.AuditError"]),
            make_class("usecase", "AuditError", &[]),
        ]);
        let resolver = LayerResolver::new(&config);
        let actual = check(&result, &resolver);
        assert_eq!(actual.len(), 1);
        assert!(matches!(
            actual[0].kind,
            ViolationKind::LayerInversion { .. }
        ));
    }

    #[test]
    fn test_layer_inversion_outer_to_inner_dep_should_not_produce_violation() {
        // usecase (rank 1) depends on domain (rank 0) — allowed
        let config = make_config(&[("domain", &["domain"]), ("usecase", &["usecase"])]);
        let result = make_result(vec![
            make_class("usecase", "AuditUseCase", &["domain.OrderEntity"]),
            make_class("domain", "OrderEntity", &[]),
        ]);
        let resolver = LayerResolver::new(&config);
        let actual = check(&result, &resolver);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_layer_inversion_same_layer_should_not_produce_violation() {
        let config = make_config(&[("domain", &["domain"])]);
        let result = make_result(vec![
            make_class("domain", "OrderEntity", &["domain.Customer"]),
            make_class("domain", "Customer", &[]),
        ]);
        let resolver = LayerResolver::new(&config);
        let actual = check(&result, &resolver);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_layer_inversion_unknown_module_should_be_skipped() {
        let config = make_config(&[("domain", &["domain"])]);
        let result = make_result(vec![make_class(
            "domain",
            "OrderEntity",
            &["stdlib.uuid.UUID"],
        )]);
        let resolver = LayerResolver::new(&config);
        let actual = check(&result, &resolver);
        assert_eq!(actual, vec![]);
    }
}
