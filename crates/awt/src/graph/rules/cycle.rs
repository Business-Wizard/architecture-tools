use std::collections::{BTreeMap, BTreeSet, HashMap};

use architecture_core::model::{ArchitectureGraph, ModuleId};
use petgraph::algo::tarjan_scc;
use petgraph::graph::DiGraph;

use crate::graph::violations::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

const MIN_CYCLE_SIZE: usize = 2;

#[must_use]
pub fn check(
    graph: &ArchitectureGraph,
    dep_map: &BTreeMap<ModuleId, BTreeSet<ModuleId>>,
) -> Vec<GraphViolation> {
    let mut index_map: HashMap<ModuleId, petgraph::graph::NodeIndex> = HashMap::new();
    let mut pg: DiGraph<ModuleId, ()> = DiGraph::new();

    for (&mid, targets) in dep_map {
        let from = *index_map.entry(mid).or_insert_with(|| pg.add_node(mid));
        for &target in targets {
            let to = *index_map
                .entry(target)
                .or_insert_with(|| pg.add_node(target));
            pg.add_edge(from, to, ());
        }
    }

    tarjan_scc(&pg)
        .into_iter()
        .filter(|scc| scc.len() >= MIN_CYCLE_SIZE)
        .map(|scc| {
            let mut modules: Vec<String> = scc
                .iter()
                .map(|&idx| graph.modules[&pg[idx]].name().0.clone())
                .collect();
            modules.sort();
            let message = format!("cycle: {}", modules.join(" → "));
            GraphViolation {
                rule: GraphRuleId::CyclicDependency,
                severity: GraphSeverity::Error,
                message,
                kind: ViolationKind::CyclicDependency { modules },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::rules::make_graph;

    #[test]
    fn test_two_modules_with_mutual_dep_should_report_cycle() {
        let graph = make_graph(&[("a", "b"), ("b", "a")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].rule, GraphRuleId::CyclicDependency);
        assert_eq!(actual[0].severity, GraphSeverity::Error);
    }

    #[test]
    fn test_three_module_cycle_should_report_one_violation() {
        let graph = make_graph(&[("a", "b"), ("b", "c"), ("c", "a")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual.len(), 1);
        let ViolationKind::CyclicDependency { modules } = &actual[0].kind else {
            panic!("wrong kind");
        };
        assert_eq!(modules, &["a", "b", "c"]);
    }

    #[test]
    fn test_dag_with_no_cycles_should_produce_no_violations() {
        let graph = make_graph(&[("a", "b"), ("b", "c")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_self_loop_should_not_produce_violation() {
        let graph = make_graph(&[("a", "a")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_two_disjoint_cycles_should_produce_two_violations() {
        let graph = make_graph(&[("a", "b"), ("b", "a"), ("c", "d"), ("d", "c")]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual.len(), 2);
    }

    #[test]
    fn test_empty_module_deps_should_produce_no_violations() {
        let graph = make_graph(&[]);
        let dep_map = super::super::module_dep_map(&graph);
        let actual = check(&graph, &dep_map);
        assert_eq!(actual, vec![]);
    }
}
