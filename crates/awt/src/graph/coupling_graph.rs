use std::collections::HashMap;

use camino::Utf8PathBuf;
use petgraph::graph::{DiGraph, NodeIndex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRole {
    Source,
    Test,
}

impl FileRole {
    pub fn from_path(path: &Utf8PathBuf) -> Self {
        let s = path.as_str();
        if s.contains("/tests/")
            || s.contains("/test_")
            || s.ends_with("_test.py")
            || s.ends_with("_test.rs")
            || s.starts_with("tests/")
            || s.starts_with("test_")
        {
            FileRole::Test
        } else {
            FileRole::Source
        }
    }
}

#[derive(Debug, Clone)]
pub struct CouplingNode {
    pub path: Utf8PathBuf,
    pub role: FileRole,
}

#[derive(Debug, Clone)]
pub struct CouplingEdge {
    pub failure_count: usize,
}

pub type CouplingGraph = DiGraph<CouplingNode, CouplingEdge>;

pub struct GraphIndex {
    pub graph: CouplingGraph,
}

fn get_or_insert_node(
    g: &mut CouplingGraph,
    map: &mut HashMap<Utf8PathBuf, NodeIndex>,
    path: Utf8PathBuf,
) -> NodeIndex {
    if let Some(&idx) = map.get(&path) {
        return idx;
    }
    let role = FileRole::from_path(&path);
    let idx = g.add_node(CouplingNode {
        path: path.clone(),
        role,
    });
    map.insert(path, idx);
    idx
}

// Try the full name first, then progressively strip leading components.
// Handles cases where `dep.from`/`dep.to` carry a root-package prefix
// (e.g. "src.domain") that is absent from source_files paths (which are
// relative to src/, so only "domain" is in the map).
fn resolve_module<'a>(
    module_map: &'a HashMap<String, Utf8PathBuf>,
    name: &str,
) -> Option<&'a Utf8PathBuf> {
    let mut s = name;
    loop {
        if let Some(v) = module_map.get(s) {
            return Some(v);
        }
        // strip one leading dotted component
        match s.find('.') {
            Some(pos) => s = &s[pos + 1..],
            None => return None,
        }
    }
}

fn build_module_map(
    source_files: &[Utf8PathBuf],
    namer: &dyn lang_core::ModuleNamer,
) -> HashMap<String, Utf8PathBuf> {
    let mut map = HashMap::new();
    for file in source_files {
        let dotted = namer.path_to_module_name(std::path::Path::new(file.as_str()));
        let parts: Vec<&str> = dotted.split('.').collect();
        for start in 0..parts.len() {
            map.entry(parts[start..].join("."))
                .or_insert_with(|| file.clone());
        }
    }
    map
}

impl GraphIndex {
    pub fn build_from_module_deps(
        deps: &[lang_core::ModuleDep],
        source_files: &[Utf8PathBuf],
        namer: &dyn lang_core::ModuleNamer,
    ) -> Self {
        let module_map = build_module_map(source_files, namer);
        let mut graph = CouplingGraph::new();
        let mut node_map: HashMap<Utf8PathBuf, NodeIndex> = HashMap::new();

        // Seed all source files so isolated nodes (no edges) appear in the graph.
        for file in source_files {
            get_or_insert_node(&mut graph, &mut node_map, file.clone());
        }

        for dep in deps {
            // dep.from imports dep.to → edge: dep.to (dependency) → dep.from (importer/depender)
            let Some(dep_file) = resolve_module(&module_map, &dep.to) else {
                continue;
            };
            let Some(importer_file) = resolve_module(&module_map, &dep.from) else {
                continue;
            };
            if dep_file == importer_file {
                continue;
            }
            let dep_idx = get_or_insert_node(&mut graph, &mut node_map, dep_file.clone());
            let imp_idx = get_or_insert_node(&mut graph, &mut node_map, importer_file.clone());
            if let Some(e) = graph.find_edge(dep_idx, imp_idx) {
                graph[e].failure_count += 1;
            } else {
                graph.add_edge(dep_idx, imp_idx, CouplingEdge { failure_count: 1 });
            }
        }

        GraphIndex { graph }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python_namer() -> py_analyzer::PythonAnalyzer {
        py_analyzer::PythonAnalyzer
    }

    #[test]
    fn test_is_test_file_with_tests_directory_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/tests/helper.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_test_prefix_in_path_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/test_utils.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_underscore_test_suffix_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/order_test.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_underscore_test_rs_suffix_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/order_test.rs"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_top_level_tests_prefix_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("tests/test_order.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_source_path_should_return_false() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/domain/order.py"));
        assert_eq!(actual, FileRole::Source);
    }

    #[test]
    fn test_build_module_map_should_include_all_dotted_suffixes() {
        let files = vec![Utf8PathBuf::from("src/domain/order.py")];
        let map = build_module_map(&files, &python_namer());
        assert_eq!(
            map.get("src.domain.order"),
            Some(&Utf8PathBuf::from("src/domain/order.py"))
        );
        assert_eq!(
            map.get("domain.order"),
            Some(&Utf8PathBuf::from("src/domain/order.py"))
        );
        assert_eq!(
            map.get("order"),
            Some(&Utf8PathBuf::from("src/domain/order.py"))
        );
    }

    #[test]
    fn test_build_module_map_init_file_should_map_to_package_name() {
        let files = vec![Utf8PathBuf::from("src/domain/__init__.py")];
        let map = build_module_map(&files, &python_namer());
        assert_eq!(
            map.get("src.domain"),
            Some(&Utf8PathBuf::from("src/domain/__init__.py"))
        );
        assert_eq!(
            map.get("domain"),
            Some(&Utf8PathBuf::from("src/domain/__init__.py"))
        );
        assert!(!map.contains_key("__init__"));
    }

    #[test]
    fn test_build_from_module_deps_should_add_edge_from_dependency_to_importer() {
        // "order" imports "billing" → edge: billing.py → order.py
        let files = vec![
            Utf8PathBuf::from("order.py"),
            Utf8PathBuf::from("billing.py"),
        ];
        let deps = vec![lang_core::ModuleDep {
            from: "order".into(),
            to: "billing".into(),
        }];
        let idx = GraphIndex::build_from_module_deps(&deps, &files, &python_namer());

        assert_eq!(idx.graph.edge_count(), 1);
        let edge = idx.graph.edge_indices().next().unwrap();
        let (src, dst) = idx.graph.edge_endpoints(edge).unwrap();
        assert_eq!(idx.graph[src].path, Utf8PathBuf::from("billing.py"));
        assert_eq!(idx.graph[dst].path, Utf8PathBuf::from("order.py"));
    }

    #[test]
    fn test_build_from_module_deps_should_skip_third_party_deps() {
        let files = vec![Utf8PathBuf::from("order.py")];
        let deps = vec![lang_core::ModuleDep {
            from: "order".into(),
            to: "requests".into(),
        }];
        let idx = GraphIndex::build_from_module_deps(&deps, &files, &python_namer());
        assert_eq!(idx.graph.edge_count(), 0);
    }

    #[test]
    fn test_build_from_module_deps_should_not_add_self_edges() {
        let files = vec![Utf8PathBuf::from("order.py")];
        let deps = vec![lang_core::ModuleDep {
            from: "order".into(),
            to: "order".into(),
        }];
        let idx = GraphIndex::build_from_module_deps(&deps, &files, &python_namer());
        assert_eq!(idx.graph.edge_count(), 0);
    }
}
