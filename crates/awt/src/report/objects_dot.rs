use std::collections::HashSet;
use std::fmt::Write as FmtWrite;
use std::io;

use camino::Utf8Path;

use architecture_core::model::{
    ArchitectureGraph, DependencyKind, ObjectId, ObjectKind, TraitLikeKind, TypeRef,
};

pub fn write_objects_dot(
    graph: &ArchitectureGraph,
    cycle_modules: &HashSet<String>,
    path: &Utf8Path,
) -> io::Result<()> {
    let dot = render(graph, cycle_modules);
    std::fs::write(path.as_std_path(), dot)
}

fn render(graph: &ArchitectureGraph, cycle_modules: &HashSet<String>) -> String {
    let connected: HashSet<ObjectId> = graph
        .dependencies
        .iter()
        .flat_map(|e| {
            let mut ids = vec![e.source];
            if let TypeRef::Internal(dst) = e.target {
                ids.push(dst);
            }
            ids
        })
        .collect();

    let mut out = String::new();
    writeln!(out, "digraph objects {{").unwrap();
    writeln!(out, "    rankdir=LR;").unwrap();

    for obj in graph.objects.values() {
        let label = obj.name.0.replace('"', "\\\"");
        let module_name = graph
            .modules
            .get(&obj.module_id)
            .map_or("", |m| m.name().0.as_str());
        let in_cycle = cycle_modules.contains(module_name);
        let is_isolated = !connected.contains(&obj.id);

        let attrs = if in_cycle {
            match obj.kind {
                ObjectKind::TraitLike(_) => {
                    "shape=ellipse style=filled fillcolor=lightcoral color=crimson"
                }
                _ => "shape=box style=filled fillcolor=lightcoral",
            }
        } else {
            match obj.kind {
                ObjectKind::TraitLike(TraitLikeKind::Protocol | TraitLikeKind::Interface) => {
                    "shape=ellipse style=filled fillcolor=lightblue"
                }
                ObjectKind::TraitLike(_) => "shape=ellipse style=filled fillcolor=lightyellow",
                _ if is_isolated => "shape=box style=filled fillcolor=yellow",
                _ => "shape=box",
            }
        };

        writeln!(out, "    {} [{attrs} label=\"{label}\"];", obj.id.0).unwrap();
    }

    for e in &graph.dependencies {
        let TypeRef::Internal(dst_id) = e.target else {
            continue;
        };
        if e.source == dst_id {
            continue;
        }
        let style = if e.kind == DependencyKind::Inherits {
            "style=dashed"
        } else {
            "style=solid"
        };
        writeln!(out, "    {} -> {} [{style}];", e.source.0, dst_id.0).unwrap();
    }

    writeln!(out, "}}").unwrap();
    out
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use architecture_core::model::{
        ArchitectureGraph, CodeObject, DependencyEdge, Module, ModuleId, ObjectId, ObjectKind,
        QualifiedName, TraitLikeKind, TypeKind, TypeRef,
    };

    use super::*;

    fn make_graph(
        objects: Vec<(ObjectId, ModuleId, &str, ObjectKind)>,
        deps: Vec<(ObjectId, ObjectId, DependencyKind)>,
    ) -> ArchitectureGraph {
        let mut module_map = BTreeMap::new();
        let mut obj_map = BTreeMap::new();
        for (oid, mid, name, kind) in objects {
            module_map.entry(mid).or_insert_with(|| Module::Source {
                id: mid,
                name: QualifiedName(format!("mod{}", mid.0)),
                file_path: format!("mod{}.py", mid.0).into(),
                object_ids: BTreeSet::new(),
            });
            obj_map.insert(
                oid,
                CodeObject {
                    id: oid,
                    module_id: mid,
                    name: QualifiedName(name.to_owned()),
                    kind,
                    constructors: vec![],
                    operations: vec![],
                },
            );
        }
        let dep_edges = deps
            .into_iter()
            .map(|(src, dst, kind)| DependencyEdge {
                source: src,
                target: TypeRef::Internal(dst),
                kind,
                occurrence_count: 1,
            })
            .collect();
        ArchitectureGraph {
            modules: module_map,
            objects: obj_map,
            dependencies: dep_edges,
            module_edges: vec![],
        }
    }

    #[test]
    fn test_render_isolated_node_should_get_yellow_fill() {
        let graph = make_graph(
            vec![(
                ObjectId(0),
                ModuleId(0),
                "domain.Order",
                ObjectKind::Type(TypeKind::Class),
            )],
            vec![],
        );
        let dot = render(&graph, &HashSet::new());
        assert!(dot.contains("fillcolor=yellow"));
    }

    #[test]
    fn test_render_interface_node_should_get_ellipse_lightblue() {
        let graph = make_graph(
            vec![(
                ObjectId(0),
                ModuleId(0),
                "domain.Repo",
                ObjectKind::TraitLike(TraitLikeKind::Protocol),
            )],
            vec![],
        );
        let dot = render(&graph, &HashSet::new());
        assert!(dot.contains("shape=ellipse") && dot.contains("fillcolor=lightblue"));
    }

    #[test]
    fn test_render_cycle_module_node_should_get_lightcoral() {
        let graph = make_graph(
            vec![(
                ObjectId(0),
                ModuleId(0),
                "domain.Service",
                ObjectKind::Type(TypeKind::Class),
            )],
            vec![],
        );
        let mut cycle_modules = HashSet::new();
        cycle_modules.insert("mod0".to_string());
        let dot = render(&graph, &cycle_modules);
        assert!(dot.contains("fillcolor=lightcoral"));
    }

    #[test]
    fn test_render_inherits_edge_should_be_dashed() {
        let graph = make_graph(
            vec![
                (
                    ObjectId(0),
                    ModuleId(0),
                    "domain.Base",
                    ObjectKind::TraitLike(TraitLikeKind::AbstractClass),
                ),
                (
                    ObjectId(1),
                    ModuleId(0),
                    "domain.Service",
                    ObjectKind::Type(TypeKind::Class),
                ),
            ],
            vec![(ObjectId(1), ObjectId(0), DependencyKind::Inherits)],
        );
        let dot = render(&graph, &HashSet::new());
        assert!(dot.contains("style=dashed"));
    }

    #[test]
    fn test_render_uses_edge_should_be_solid() {
        let graph = make_graph(
            vec![
                (
                    ObjectId(0),
                    ModuleId(0),
                    "domain.Order",
                    ObjectKind::Type(TypeKind::Class),
                ),
                (
                    ObjectId(1),
                    ModuleId(0),
                    "domain.Service",
                    ObjectKind::Type(TypeKind::Class),
                ),
            ],
            vec![(ObjectId(1), ObjectId(0), DependencyKind::Calls)],
        );
        let dot = render(&graph, &HashSet::new());
        assert!(dot.contains("style=solid"));
    }
}
