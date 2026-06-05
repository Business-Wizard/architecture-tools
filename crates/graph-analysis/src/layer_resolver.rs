use crate::config::{GraphLayerConfig, LayerDef};

pub struct LayerResolver<'a> {
    /// Sorted entries: (prefix, `layer_index`, `layer_name`) by prefix length descending.
    entries: Vec<(&'a str, usize, &'a str)>,
}

impl<'a> LayerResolver<'a> {
    pub fn new(config: &'a GraphLayerConfig) -> Self {
        let mut entries: Vec<(&'a str, usize, &'a str)> = config
            .layers
            .iter()
            .enumerate()
            .flat_map(|(idx, layer): (usize, &'a LayerDef)| {
                layer
                    .module_prefixes
                    .iter()
                    .map(move |prefix| (prefix.as_str(), idx, layer.name.as_str()))
            })
            .collect();
        // Longest prefix first so first match is most specific.
        entries.sort_by_key(|e| std::cmp::Reverse(e.0.len()));
        Self { entries }
    }

    /// Returns `(layer_index, layer_name)` for the given dotted module name.
    /// Uses longest-prefix match; returns `None` if no prefix matches.
    pub fn resolve(&self, module: &str) -> Option<(usize, &str)> {
        self.entries.iter().find_map(|(prefix, idx, name)| {
            if module == *prefix || module.starts_with(&format!("{prefix}.")) {
                Some((*idx, *name))
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LayerDef;

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

    #[test]
    fn test_exact_prefix_match_should_resolve_layer() {
        let config = make_config(&[("domain", &["domain"])]);
        let resolver = LayerResolver::new(&config);
        let actual = resolver.resolve("domain");
        let expected = Some((0usize, "domain"));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_dotted_sub_module_should_resolve_to_matching_layer() {
        let config = make_config(&[("domain", &["domain"]), ("usecase", &["usecase"])]);
        let resolver = LayerResolver::new(&config);
        let actual = resolver.resolve("domain.order");
        let expected = Some((0usize, "domain"));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_longest_prefix_wins_over_shorter_match_should_return_specific_layer() {
        let config = make_config(&[
            ("infra", &["feature"]),
            ("postgres", &["feature.postgres_repo"]),
        ]);
        let resolver = LayerResolver::new(&config);
        let actual = resolver.resolve("feature.postgres_repo.audit");
        let expected = Some((1usize, "postgres"));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_no_prefix_match_should_return_none() {
        let config = make_config(&[("domain", &["domain"])]);
        let resolver = LayerResolver::new(&config);
        let actual = resolver.resolve("stdlib.os");
        assert_eq!(actual, None);
    }

    #[test]
    fn test_partial_prefix_name_should_not_match() {
        let config = make_config(&[("infra", &["feature.postgres_repo"])]);
        let resolver = LayerResolver::new(&config);
        // "feature.postgres" is not "feature.postgres_repo" — must not match
        let actual = resolver.resolve("feature.postgres");
        assert_eq!(actual, None);
    }
}
