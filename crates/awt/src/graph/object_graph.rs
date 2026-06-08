use std::collections::HashMap;

use petgraph::graph::{DiGraph, NodeIndex};

use lang_core::ClassDef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectKind {
    Interface,
    TraitLike,
    Concrete,
}

impl ObjectKind {
    fn from_bases(bases: &[String]) -> Self {
        for base in bases {
            let short = base.split('.').last().unwrap_or(base.as_str());
            match short {
                "Protocol" => return Self::Interface,
                "ABC" | "ABCMeta" => return Self::TraitLike,
                _ => {}
            }
        }
        Self::Concrete
    }
}

#[derive(Debug, Clone)]
pub struct ObjectNode {
    pub qualified_name: String,
    pub module: String,
    pub kind: ObjectKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeKind {
    Inherits,
    Uses,
}

#[derive(Debug, Clone)]
pub struct ObjectEdge {
    pub kind: EdgeKind,
}

pub type ObjectGraph = DiGraph<ObjectNode, ObjectEdge>;

pub struct ObjectGraphIndex {
    pub graph: ObjectGraph,
    /// Maps qualified_name → NodeIndex for external lookups.
    pub node_map: HashMap<String, NodeIndex>,
}

impl ObjectGraphIndex {
    pub fn build_from_class_defs(class_defs: &[ClassDef]) -> Self {
        let mut graph = ObjectGraph::new();
        let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

        for def in class_defs {
            let qname = format!("{}.{}", def.module, def.name);
            let idx = graph.add_node(ObjectNode {
                qualified_name: qname.clone(),
                module: def.module.clone(),
                kind: ObjectKind::from_bases(&def.bases),
            });
            node_map.insert(qname, idx);
        }

        for def in class_defs {
            let src_qname = format!("{}.{}", def.module, def.name);
            let Some(&src) = node_map.get(&src_qname) else {
                continue;
            };

            for base in &def.bases {
                if let Some(&dst) = node_map.get(base) {
                    graph.add_edge(
                        src,
                        dst,
                        ObjectEdge {
                            kind: EdgeKind::Inherits,
                        },
                    );
                }
            }

            for dep in &def.class_deps {
                if let Some(&dst) = node_map.get(dep) {
                    if dst != src {
                        graph.add_edge(
                            src,
                            dst,
                            ObjectEdge {
                                kind: EdgeKind::Uses,
                            },
                        );
                    }
                }
            }
        }

        Self { graph, node_map }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_core::ClassDef;

    fn make_def(module: &str, name: &str, bases: Vec<&str>, deps: Vec<&str>) -> ClassDef {
        ClassDef {
            module: module.to_string(),
            name: name.to_string(),
            bases: bases.into_iter().map(str::to_string).collect(),
            attributes: vec![],
            methods: vec![],
            class_deps: deps.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn test_build_from_single_class_should_produce_one_node() {
        let defs = vec![make_def("domain", "Order", vec![], vec![])];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        assert_eq!(idx.graph.node_count(), 1);
        assert_eq!(idx.graph.edge_count(), 0);
    }

    #[test]
    fn test_protocol_base_should_set_kind_interface() {
        let defs = vec![make_def("domain", "Repo", vec!["Protocol"], vec![])];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        let node = &idx.graph[idx.node_map["domain.Repo"]];
        assert_eq!(node.kind, ObjectKind::Interface);
    }

    #[test]
    fn test_abc_base_should_set_kind_traitlike() {
        let defs = vec![make_def("domain", "Base", vec!["ABC"], vec![])];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        let node = &idx.graph[idx.node_map["domain.Base"]];
        assert_eq!(node.kind, ObjectKind::TraitLike);
    }

    #[test]
    fn test_class_dep_within_graph_should_produce_uses_edge() {
        let defs = vec![
            make_def("domain", "Order", vec![], vec![]),
            make_def("domain", "Service", vec![], vec!["domain.Order"]),
        ];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        assert_eq!(idx.graph.edge_count(), 1);
        let edge = idx.graph.raw_edges().first().unwrap();
        assert_eq!(edge.weight.kind, EdgeKind::Uses);
    }

    #[test]
    fn test_inherits_edge_resolves_internal_base() {
        let defs = vec![
            make_def("domain", "Base", vec!["ABC"], vec![]),
            make_def("domain", "Service", vec!["domain.Base"], vec![]),
        ];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        let inherits = idx
            .graph
            .raw_edges()
            .iter()
            .filter(|e| e.weight.kind == EdgeKind::Inherits)
            .count();
        assert_eq!(inherits, 1);
    }
}
