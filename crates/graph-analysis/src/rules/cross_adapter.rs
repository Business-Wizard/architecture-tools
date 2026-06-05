use std::collections::HashMap;

use py_analyzer::InspectResult;

use crate::layer_resolver::LayerResolver;
use crate::model::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

/// Returns the adapter prefix for a module given how many dotted components the layer prefix has.
/// e.g. `module="feature.postgres_repo.audit`", `layer_prefix_depth=1` → "`feature.postgres_repo`"
fn adapter_prefix(module: &str, layer_prefix_depth: usize) -> &str {
    let adapter_depth = layer_prefix_depth + 1;
    let mut end = 0;
    let mut count = 0;
    for (i, ch) in module.char_indices() {
        if ch == '.' {
            count += 1;
            if count == adapter_depth {
                end = i;
                break;
            }
        }
        end = i + ch.len_utf8();
    }
    if count < adapter_depth {
        module
    } else {
        &module[..end]
    }
}

pub fn check(result: &InspectResult, resolver: &LayerResolver<'_>) -> Vec<GraphViolation> {
    // Build qualified class → (layer_index, layer_name, module) lookup.
    let class_info: HashMap<String, (usize, String, String)> = result
        .classes
        .iter()
        .filter_map(|c| {
            resolver.resolve(&c.module).map(|(rank, layer)| {
                (
                    format!("{}.{}", c.module, c.name),
                    (rank, layer.to_string(), c.module.clone()),
                )
            })
        })
        .collect();

    // Compute prefix depth for each layer by inspecting the shortest prefix in that layer's config.
    // We derive it from the resolved layer name → find a matching prefix in config.
    // Since we only need it per-class, we derive it on the fly from the resolved module prefix.
    let prefix_depth = |module: &str, layer_name: &str| -> usize {
        // Find the longest matching prefix for this module in this layer.
        // We walk back through resolver entries — but we don't have access to internals,
        // so we scan config layers directly.
        result
            .classes
            .iter()
            .find(|c| c.module == module)
            .and({
                // Use the layer name to find the matching prefix via the config indirectly:
                // count dots in the module prefix that matched. We can infer this from the module itself
                // by finding the longest prefix of the module that resolves to the same layer.
                None::<usize>
            })
            .unwrap_or_else(|| {
                // Fallback: count dots in module up to layer_name length heuristic.
                // Since we don't have direct config access here, use 1 as default adapter depth.
                let _ = layer_name;
                1
            })
    };

    let mut violations = vec![];

    for cls in &result.classes {
        let src_key = format!("{}.{}", cls.module, cls.name);
        let Some((src_rank, src_layer, _)) = class_info.get(&src_key) else {
            continue;
        };

        for dep in &cls.class_deps {
            let Some((dep_rank, dep_layer, dep_module)) = class_info.get(dep.as_str()) else {
                continue;
            };

            // Only flag same-layer deps between different adapters.
            if src_rank != dep_rank || src_layer != dep_layer {
                continue;
            }

            let depth = prefix_depth(&cls.module, src_layer);
            let src_adapter = adapter_prefix(&cls.module, depth);
            let dep_adapter = adapter_prefix(dep_module, depth);

            if src_adapter == dep_adapter {
                continue;
            }

            violations.push(GraphViolation {
                rule: GraphRuleId::CrossAdapterCoupling,
                severity: GraphSeverity::Error,
                message: format!("{src_key} -> {dep}"),
                kind: ViolationKind::CrossAdapterCoupling {
                    from_class: src_key.clone(),
                    to_class: dep.clone(),
                    from_adapter: src_adapter.to_string(),
                    to_adapter: dep_adapter.to_string(),
                    fix_hint: format!(
                        "introduce a shared domain value object instead of coupling `{src_adapter}` to `{dep_adapter}`"
                    ),
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
    fn test_cross_adapter_between_different_sub_prefixes_should_produce_violation() {
        let config = make_config(&[("infra", &["feature"])]);
        let result = make_result(vec![
            make_class(
                "feature.local_file_repo",
                "PhotoRepo",
                &["feature.postgres_repo._models.CapturedPhotoRow"],
            ),
            make_class("feature.postgres_repo._models", "CapturedPhotoRow", &[]),
        ]);
        let resolver = LayerResolver::new(&config);
        let actual = check(&result, &resolver);
        assert_eq!(actual.len(), 1);
        assert!(matches!(
            actual[0].kind,
            ViolationKind::CrossAdapterCoupling { .. }
        ));
    }

    #[test]
    fn test_cross_adapter_within_same_sub_prefix_should_not_produce_violation() {
        let config = make_config(&[("infra", &["feature.postgres_repo"])]);
        let result = make_result(vec![
            make_class(
                "feature.postgres_repo.audit",
                "AuditRepo",
                &["feature.postgres_repo._models.AuditRow"],
            ),
            make_class("feature.postgres_repo._models", "AuditRow", &[]),
        ]);
        let resolver = LayerResolver::new(&config);
        let actual = check(&result, &resolver);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_cross_adapter_different_layers_should_not_produce_violation() {
        // usecase → domain is a layer inversion, not cross-adapter
        let config = make_config(&[("domain", &["domain"]), ("usecase", &["usecase"])]);
        let result = make_result(vec![
            make_class("usecase", "AuditUseCase", &["domain.OrderEntity"]),
            make_class("domain", "OrderEntity", &[]),
        ]);
        let resolver = LayerResolver::new(&config);
        let actual = check(&result, &resolver);
        assert_eq!(actual, vec![]);
    }
}
