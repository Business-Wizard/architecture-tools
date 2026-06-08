use std::collections::HashSet;
use std::fmt::Write as FmtWrite;
use std::io;

use camino::Utf8Path;

use crate::graph::object_graph::{EdgeKind, ObjectGraphIndex, ObjectKind};

pub fn write_objects_dot(
    idx: &ObjectGraphIndex,
    cycle_modules: &HashSet<String>,
    path: &Utf8Path,
) -> io::Result<()> {
    let dot = render(idx, cycle_modules);
    std::fs::write(path.as_std_path(), dot)
}

fn render(idx: &ObjectGraphIndex, cycle_modules: &HashSet<String>) -> String {
    let mut out = String::new();
    writeln!(out, "digraph objects {{").unwrap();
    writeln!(out, "    rankdir=LR;").unwrap();

    for n in idx.graph.node_indices() {
        let node = &idx.graph[n];
        let label = node.qualified_name.replace('"', "\\\"");
        let in_cycle = cycle_modules.contains(&node.module);
        let is_isolated = idx.graph.edges(n).count() == 0
            && idx
                .graph
                .edges_directed(n, petgraph::Direction::Incoming)
                .count()
                == 0;

        let attrs = if in_cycle {
            match node.kind {
                ObjectKind::Interface | ObjectKind::TraitLike => {
                    "shape=ellipse style=filled fillcolor=lightcoral color=crimson"
                }
                ObjectKind::Concrete => "shape=box style=filled fillcolor=lightcoral",
            }
        } else {
            match node.kind {
                ObjectKind::Interface => "shape=ellipse style=filled fillcolor=lightblue",
                ObjectKind::TraitLike => "shape=ellipse style=filled fillcolor=lightyellow",
                ObjectKind::Concrete if is_isolated => "shape=box style=filled fillcolor=yellow",
                ObjectKind::Concrete => "shape=box",
            }
        };

        writeln!(out, "    {} [{attrs} label=\"{label}\"];", n.index()).unwrap();
    }

    for e in idx.graph.edge_indices() {
        let (src, dst) = idx.graph.edge_endpoints(e).unwrap();
        let style = match idx.graph[e].kind {
            EdgeKind::Inherits => "style=dashed",
            EdgeKind::Uses => "style=solid",
        };
        writeln!(out, "    {} -> {} [{style}];", src.index(), dst.index()).unwrap();
    }

    writeln!(out, "}}").unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::object_graph::ObjectGraphIndex;
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
    fn test_render_isolated_node_should_get_yellow_fill() {
        let defs = vec![make_def("domain", "Order", vec![], vec![])];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        let dot = render(&idx, &HashSet::new());
        assert!(dot.contains("fillcolor=yellow"));
    }

    #[test]
    fn test_render_interface_node_should_get_ellipse_lightblue() {
        let defs = vec![make_def("domain", "Repo", vec!["Protocol"], vec![])];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        let dot = render(&idx, &HashSet::new());
        assert!(dot.contains("shape=ellipse") && dot.contains("fillcolor=lightblue"));
    }

    #[test]
    fn test_render_cycle_module_node_should_get_lightcoral() {
        let defs = vec![make_def("domain", "Service", vec![], vec![])];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        let mut cycle_modules = HashSet::new();
        cycle_modules.insert("domain".to_string());
        let dot = render(&idx, &cycle_modules);
        assert!(dot.contains("fillcolor=lightcoral"));
    }

    #[test]
    fn test_render_inherits_edge_should_be_dashed() {
        let defs = vec![
            make_def("domain", "Base", vec!["ABC"], vec![]),
            make_def("domain", "Service", vec!["domain.Base"], vec![]),
        ];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        let dot = render(&idx, &HashSet::new());
        assert!(dot.contains("style=dashed"));
    }

    #[test]
    fn test_render_uses_edge_should_be_solid() {
        let defs = vec![
            make_def("domain", "Order", vec![], vec![]),
            make_def("domain", "Service", vec![], vec!["domain.Order"]),
        ];
        let idx = ObjectGraphIndex::build_from_class_defs(&defs);
        let dot = render(&idx, &HashSet::new());
        assert!(dot.contains("style=solid"));
    }
}
