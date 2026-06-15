mod cycle;
mod god_module;
mod module_hub;

use std::collections::{BTreeMap, BTreeSet};

use architecture_core::model::{ArchitectureGraph, ModuleId};

use crate::graph::violations::{GraphSeverity, GraphViolation};

#[must_use]
pub fn run_all(graph: &ArchitectureGraph) -> Vec<GraphViolation> {
    let dep_map = module_dep_map(graph);
    let mut violations = vec![];
    violations.extend(cycle::check(graph, &dep_map));
    violations.extend(module_hub::check(graph, &dep_map));
    violations.extend(god_module::check(graph, &dep_map));
    violations.sort_by(|a, b| {
        let sev_ord = severity_ord(&b.severity).cmp(&severity_ord(&a.severity));
        sev_ord.then_with(|| a.message.cmp(&b.message))
    });
    violations
}

fn module_dep_map(graph: &ArchitectureGraph) -> BTreeMap<ModuleId, BTreeSet<ModuleId>> {
    let mut map: BTreeMap<ModuleId, BTreeSet<ModuleId>> = BTreeMap::new();
    for &mid in graph.modules.keys() {
        map.entry(mid).or_default();
    }
    for edge in &graph.module_edges {
        if edge.from != edge.to {
            map.entry(edge.from).or_default().insert(edge.to);
        }
    }
    map
}

fn severity_ord(s: &GraphSeverity) -> u8 {
    match s {
        GraphSeverity::Error => 1,
        GraphSeverity::Warning => 0,
    }
}

#[cfg(test)]
pub(crate) fn make_graph(pairs: &[(&str, &str)]) -> ArchitectureGraph {
    use std::collections::BTreeSet;

    use architecture_core::model::{Module, ModuleEdge, QualifiedName};

    let mut modules = BTreeMap::new();
    let mut name_to_id: BTreeMap<&str, ModuleId> = BTreeMap::new();
    let all_names: BTreeSet<&str> = pairs.iter().flat_map(|(a, b)| [*a, *b]).collect();
    for (next_id, name) in all_names.iter().enumerate() {
        let id = ModuleId(u32::try_from(next_id).expect("name count fits u32"));
        name_to_id.insert(name, id);
        modules.insert(
            id,
            Module::Source {
                id,
                name: QualifiedName((*name).to_string()),
                file_path: format!("{name}.py").into(),
                object_ids: BTreeSet::new(),
            },
        );
    }

    let module_edges = pairs
        .iter()
        .map(|(f, t)| ModuleEdge {
            from: name_to_id[f],
            to: name_to_id[t],
        })
        .collect();

    ArchitectureGraph {
        modules,
        objects: BTreeMap::new(),
        dependencies: vec![],
        module_edges,
    }
}
