use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphLayerConfig {
    /// Ordered most-stable (index 0) → least-stable (last).
    pub layers: Vec<LayerDef>,
    /// Classes referenced by more than this many distinct classes are flagged.
    pub fan_in_threshold: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDef {
    pub name: String,
    /// Dotted-module prefix patterns. Longest-prefix match wins.
    pub module_prefixes: Vec<String>,
}

impl Default for GraphLayerConfig {
    fn default() -> Self {
        Self {
            layers: vec![],
            fan_in_threshold: 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_layer_config_default_should_have_empty_layers_and_threshold_8() {
        let cfg = GraphLayerConfig::default();
        let expected = GraphLayerConfig {
            layers: vec![],
            fan_in_threshold: 8,
        };
        assert_eq!(cfg.layers.len(), expected.layers.len());
        assert_eq!(cfg.fan_in_threshold, expected.fan_in_threshold);
    }
}
