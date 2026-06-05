use std::collections::HashMap;

use camino::Utf8PathBuf;
use petgraph::graph::NodeIndex;

use crate::config::LayerConfig;
use crate::graph::coupling_graph::GraphIndex;

pub struct LayerMap(HashMap<NodeIndex, String>);

impl LayerMap {
    pub fn get_layer(&self, idx: NodeIndex) -> Option<&str> {
        self.0.get(&idx).map(String::as_str)
    }
}

pub fn build_layer_map(graph_idx: &GraphIndex, layers: &[LayerConfig]) -> LayerMap {
    let mut map = HashMap::new();
    for n in graph_idx.graph.node_indices() {
        let path = &graph_idx.graph[n].path;
        if let Some(layer) = find_layer(path, layers) {
            map.insert(n, layer.name.clone());
        }
    }
    LayerMap(map)
}

fn find_layer<'a>(path: &Utf8PathBuf, layers: &'a [LayerConfig]) -> Option<&'a LayerConfig> {
    layers.iter().find(|layer| {
        layer.paths.iter().any(|pattern| {
            let prefix = pattern.trim_end_matches("/**").trim_end_matches("/*");
            path.as_str().starts_with(prefix)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_layer(name: &str, paths: &[&str]) -> LayerConfig {
        LayerConfig {
            name: name.to_string(),
            paths: paths.iter().map(ToString::to_string).collect(),
            may_depend_on: vec![],
        }
    }

    #[test]
    fn test_find_layer_should_match_path_under_glob_prefix() {
        let layers = vec![stub_layer("domain", &["src/domain/**"])];
        let path = Utf8PathBuf::from("src/domain/order.py");
        let actual = find_layer(&path, &layers).map(|l| l.name.as_str());
        assert_eq!(actual, Some("domain"));
    }

    #[test]
    fn test_find_layer_should_return_none_when_no_match() {
        let layers = vec![stub_layer("domain", &["src/domain/**"])];
        let path = Utf8PathBuf::from("src/usecases/order_service.py");
        let actual = find_layer(&path, &layers);
        assert!(actual.is_none());
    }

    #[test]
    fn test_find_layer_should_return_first_match() {
        let layers = vec![
            stub_layer("domain", &["src/domain/**"]),
            stub_layer("other", &["src/domain/**"]),
        ];
        let path = Utf8PathBuf::from("src/domain/entity.py");
        let actual = find_layer(&path, &layers).map(|l| l.name.as_str());
        assert_eq!(actual, Some("domain"));
    }
}
