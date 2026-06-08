use std::collections::HashMap;

use lang_core::ModuleDep;
use petgraph::algo::tarjan_scc;
use petgraph::graph::DiGraph;

use crate::model::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

const MIN_CYCLE_SIZE: usize = 2;

#[must_use]
pub fn check(deps: &[ModuleDep]) -> Vec<GraphViolation> {
    let mut index_map: HashMap<&str, petgraph::graph::NodeIndex> = HashMap::new();
    let mut graph: DiGraph<&str, ()> = DiGraph::new();

    for dep in deps {
        let from = *index_map
            .entry(dep.from.as_str())
            .or_insert_with(|| graph.add_node(dep.from.as_str()));
        let to = *index_map
            .entry(dep.to.as_str())
            .or_insert_with(|| graph.add_node(dep.to.as_str()));
        graph.add_edge(from, to, ());
    }

    tarjan_scc(&graph)
        .into_iter()
        .filter(|scc| scc.len() >= MIN_CYCLE_SIZE)
        .map(|scc| {
            let mut modules: Vec<String> = scc.iter().map(|&idx| graph[idx].to_string()).collect();
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

    fn make_deps(pairs: &[(&str, &str)]) -> Vec<ModuleDep> {
        pairs
            .iter()
            .map(|(f, t)| ModuleDep {
                from: (*f).into(),
                to: (*t).into(),
            })
            .collect()
    }

    #[test]
    fn test_two_modules_with_mutual_dep_should_report_cycle() {
        let actual = check(&make_deps(&[("a", "b"), ("b", "a")]));
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].rule, GraphRuleId::CyclicDependency);
        assert_eq!(actual[0].severity, GraphSeverity::Error);
    }

    #[test]
    fn test_three_module_cycle_should_report_one_violation() {
        let actual = check(&make_deps(&[("a", "b"), ("b", "c"), ("c", "a")]));
        assert_eq!(actual.len(), 1);
        let ViolationKind::CyclicDependency { modules } = &actual[0].kind else {
            panic!("wrong kind");
        };
        assert_eq!(modules, &["a", "b", "c"]);
    }

    #[test]
    fn test_dag_with_no_cycles_should_produce_no_violations() {
        let actual = check(&make_deps(&[("a", "b"), ("b", "c")]));
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_self_loop_should_not_produce_violation() {
        // A single node in its own SCC is not a cycle between modules.
        let actual = check(&make_deps(&[("a", "a")]));
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_two_disjoint_cycles_should_produce_two_violations() {
        let actual = check(&make_deps(&[
            ("a", "b"),
            ("b", "a"),
            ("c", "d"),
            ("d", "c"),
        ]));
        assert_eq!(actual.len(), 2);
    }

    #[test]
    fn test_empty_module_deps_should_produce_no_violations() {
        let actual = check(&make_deps(&[]));
        assert_eq!(actual, vec![]);
    }
}
