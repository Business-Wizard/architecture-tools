use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::io;

use camino::Utf8Path;
use petgraph::algo::tarjan_scc;
use petgraph::graph::DiGraph;

use architecture_core::model::{ArchitectureGraph, ModuleId};

use crate::graph::metrics::MetricsResult;

pub fn write_dot(
    graph: &ArchitectureGraph,
    metrics: &MetricsResult,
    path: &Utf8Path,
) -> io::Result<()> {
    let dot = render(graph, metrics);
    std::fs::write(path.as_std_path(), dot)
}

fn cycle_module_ids(graph: &ArchitectureGraph) -> HashSet<ModuleId> {
    let source_ids: Vec<ModuleId> = graph
        .modules
        .values()
        .filter(|m| !m.is_test())
        .map(architecture_core::model::Module::id)
        .collect();

    let mut pg: DiGraph<ModuleId, ()> = DiGraph::new();
    let mut id_to_ni = HashMap::new();
    for &mid in &source_ids {
        let ni = pg.add_node(mid);
        id_to_ni.insert(mid, ni);
    }
    for e in &graph.module_edges {
        if let (Some(&s), Some(&d)) = (id_to_ni.get(&e.from), id_to_ni.get(&e.to))
            && e.from != e.to
        {
            pg.add_edge(s, d, ());
        }
    }
    tarjan_scc(&pg)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .flatten()
        .map(|ni| pg[ni])
        .collect()
}

/// Returns the set of module name prefixes (directory stems) for all files in a coupling cycle.
/// Used to cross-highlight object nodes whose containing module participates in a file-level cycle.
pub fn cycle_module_names(graph: &ArchitectureGraph) -> HashSet<String> {
    cycle_module_ids(graph)
        .into_iter()
        .filter_map(|mid| {
            let path = graph.modules.get(&mid)?.file_path();
            path.parent().map(|p| p.as_str().replace('/', "."))
        })
        .collect()
}

fn penwidth(count: usize) -> f32 {
    // 1.0 at count=1, grows with sqrt to avoid runaway thickness
    // count is always small in practice; cap to avoid any precision concern
    let capped = u32::try_from(count).unwrap_or(u32::MAX);
    1.0_f32 + f32::from(u16::try_from(capped).unwrap_or(u16::MAX)).sqrt()
}

fn render(graph: &ArchitectureGraph, metrics: &MetricsResult) -> String {
    let cycles = cycle_module_ids(graph);

    let source_ids: HashSet<ModuleId> = graph
        .modules
        .values()
        .filter(|m| !m.is_test())
        .map(architecture_core::model::Module::id)
        .collect();

    let instability_map: HashMap<_, f64> = metrics
        .nodes
        .iter()
        .map(|n| (n.file.as_path(), n.instability.as_f64()))
        .collect();

    // Count how many times each ModuleId appears as an endpoint (source modules only)
    let mut endpoint_count: HashMap<ModuleId, usize> = HashMap::new();
    let mut edge_counts: HashMap<(ModuleId, ModuleId), usize> = HashMap::new();
    for e in &graph.module_edges {
        if !source_ids.contains(&e.from) || !source_ids.contains(&e.to) {
            continue;
        }
        *endpoint_count.entry(e.from).or_insert(0) += 1;
        *endpoint_count.entry(e.to).or_insert(0) += 1;
        *edge_counts.entry((e.from, e.to)).or_insert(0) += 1;
    }

    let mut out = String::new();
    writeln!(out, "digraph coupling {{").unwrap();
    writeln!(out, "    rankdir=RL;").unwrap();

    // Iterate in stable BTreeMap order for deterministic output
    for module in graph.modules.values() {
        if module.is_test() {
            continue;
        }
        let mid = module.id();
        let i = instability_map
            .get(module.file_path())
            .copied()
            .unwrap_or(0.0);
        let label = format!(
            "{}\\nI={:.2}",
            module.file_path().as_str().replace('"', "\\\""),
            i
        );
        let is_isolated = endpoint_count.get(&mid).copied().unwrap_or(0) == 0;
        let attrs = if cycles.contains(&mid) {
            "shape=box style=filled fillcolor=lightcoral"
        } else if is_isolated {
            "shape=box style=filled fillcolor=yellow"
        } else {
            "shape=box"
        };
        writeln!(out, "    {} [{attrs} label=\"{label}\"];", mid.0).unwrap();
    }

    for ((from, to), count) in &edge_counts {
        let pw = penwidth(*count);
        let cycle_edge = cycles.contains(from) && cycles.contains(to);
        let color_attr = if cycle_edge { " color=crimson" } else { "" };
        writeln!(
            out,
            "    {} -> {} [label=\"{count}\" penwidth={pw:.2}{color_attr}];",
            from.0, to.0
        )
        .unwrap();
    }

    writeln!(out, "}}").unwrap();
    out
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use architecture_core::model::{
        ArchitectureGraph, Module, ModuleEdge, ModuleId, QualifiedName,
    };

    use super::*;
    use crate::graph::metrics;
    use camino::Utf8PathBuf;

    fn stub_metrics(graph: &ArchitectureGraph) -> MetricsResult {
        metrics::compute(graph)
    }

    fn arch_from_pairs(pairs: &[(&str, &str)]) -> ArchitectureGraph {
        use std::collections::BTreeSet as Set;
        let mut name_to_id: BTreeMap<&str, ModuleId> = BTreeMap::new();
        let all_names: Set<&str> = pairs.iter().flat_map(|(a, b)| [*a, *b]).collect();
        let mut modules = BTreeMap::new();
        for (next_id, &name) in all_names.iter().enumerate() {
            let id = ModuleId(u32::try_from(next_id).expect("fits u32"));
            name_to_id.insert(name, id);
            modules.insert(
                id,
                Module::Source {
                    id,
                    name: QualifiedName(name.to_owned()),
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

    fn arch_with_test_module(
        source_pairs: &[(&str, &str)],
        test_names: &[&str],
        test_pairs: &[(&str, &str)],
    ) -> ArchitectureGraph {
        use std::collections::BTreeSet as Set;
        let all_names: Set<&str> = source_pairs
            .iter()
            .flat_map(|(a, b)| [*a, *b])
            .chain(test_names.iter().copied())
            .chain(test_pairs.iter().flat_map(|(a, b)| [*a, *b]))
            .collect();

        let test_set: HashSet<&str> = test_names.iter().copied().collect();
        let mut name_to_id: BTreeMap<&str, ModuleId> = BTreeMap::new();
        let mut modules = BTreeMap::new();
        for (next_id, &name) in all_names.iter().enumerate() {
            let id = ModuleId(u32::try_from(next_id).expect("fits u32"));
            name_to_id.insert(name, id);
            let module = if test_set.contains(name) {
                Module::Test {
                    id,
                    name: QualifiedName(name.to_owned()),
                    file_path: format!("{name}.py").into(),
                    object_ids: BTreeSet::new(),
                }
            } else {
                Module::Source {
                    id,
                    name: QualifiedName(name.to_owned()),
                    file_path: format!("{name}.py").into(),
                    object_ids: BTreeSet::new(),
                }
            };
            modules.insert(id, module);
        }
        let module_edges = source_pairs
            .iter()
            .chain(test_pairs.iter())
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

    fn node_index_in_dot(dot: &str, filename: &str) -> Option<u32> {
        for line in dot.lines() {
            let trimmed = line.trim();
            if trimmed.contains(&format!("label=\"{filename}")) {
                return trimmed
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok());
            }
        }
        None
    }

    /// domain.py imports service.py
    fn fixture_one_import() -> ArchitectureGraph {
        arch_from_pairs(&[("domain", "service")])
    }

    /// `test_domain.py` imports domain.py — test node should be excluded from output
    fn fixture_test_imports_source() -> ArchitectureGraph {
        arch_with_test_module(&[], &["test_domain"], &[("test_domain", "domain")])
    }

    /// a.py and b.py mutually import each other → cycle
    fn fixture_cycle() -> ArchitectureGraph {
        arch_from_pairs(&[("a", "b"), ("b", "a")])
    }

    /// balanced.py imports domain.py; service.py imports balanced.py
    fn fixture_balanced_node() -> ArchitectureGraph {
        arch_from_pairs(&[("balanced", "domain"), ("service", "balanced")])
    }

    /// hub.py imported by a, b, c; hub imports x, y
    fn fixture_hub_node() -> ArchitectureGraph {
        arch_from_pairs(&[
            ("hub", "x"),
            ("hub", "y"),
            ("a", "hub"),
            ("b", "hub"),
            ("c", "hub"),
        ])
    }

    /// mid.py imported by consumer; mid imports x, y
    fn fixture_mid_node() -> ArchitectureGraph {
        arch_from_pairs(&[("mid", "x"), ("mid", "y"), ("consumer", "mid")])
    }

    /// consumer imports hub 4 times (e.g. different named imports resolving to same file)
    fn fixture_repeated_import() -> ArchitectureGraph {
        use std::collections::BTreeSet;
        let mut modules = BTreeMap::new();
        let hub_id = ModuleId(0);
        let consumer_id = ModuleId(1);
        modules.insert(
            hub_id,
            Module::Source {
                id: hub_id,
                name: QualifiedName("hub".into()),
                file_path: Utf8PathBuf::from("hub.py"),
                object_ids: BTreeSet::new(),
            },
        );
        modules.insert(
            consumer_id,
            Module::Source {
                id: consumer_id,
                name: QualifiedName("consumer".into()),
                file_path: Utf8PathBuf::from("consumer.py"),
                object_ids: BTreeSet::new(),
            },
        );
        let module_edges = vec![
            ModuleEdge {
                from: consumer_id,
                to: hub_id,
            },
            ModuleEdge {
                from: consumer_id,
                to: hub_id,
            },
            ModuleEdge {
                from: consumer_id,
                to: hub_id,
            },
            ModuleEdge {
                from: consumer_id,
                to: hub_id,
            },
        ];
        ArchitectureGraph {
            modules,
            objects: BTreeMap::new(),
            dependencies: vec![],
            module_edges,
        }
    }

    // ── penwidth ─────────────────────────────────────────────────────────────

    #[test]
    fn test_penwidth_at_count_one_should_be_two_point_zero() {
        assert_eq!(format!("{:.2}", penwidth(1)), "2.00");
    }

    #[test]
    fn test_penwidth_at_count_four_should_be_three_point_zero() {
        assert_eq!(format!("{:.2}", penwidth(4)), "3.00");
    }

    #[test]
    fn test_penwidth_at_count_nine_should_be_four_point_zero() {
        assert_eq!(format!("{:.2}", penwidth(9)), "4.00");
    }

    #[test]
    fn test_cycle_edges_should_get_crimson_color() {
        let graph = fixture_cycle();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("color=crimson"));
    }

    #[test]
    fn test_non_cycle_edges_should_not_get_crimson_color() {
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("color=crimson"));
    }

    #[test]
    fn test_penwidth_grows_with_count() {
        assert!(penwidth(9) > penwidth(1));
    }

    // ── structure ────────────────────────────────────────────────────────────

    #[test]
    fn test_render_should_open_with_digraph_coupling() {
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.starts_with("digraph coupling {"));
    }

    #[test]
    fn test_render_should_have_rankdir_rl() {
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("rankdir=RL;"));
    }

    #[test]
    fn test_render_should_include_exactly_two_source_nodes() {
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert_eq!(dot.matches("[shape=box").count(), 2);
    }

    // ── node labels ──────────────────────────────────────────────────────────

    #[test]
    fn test_render_should_use_box_shape_for_source_nodes() {
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("shape=box"));
    }

    #[test]
    fn test_render_stable_node_should_have_exact_label() {
        // domain imports service → fan_in(service)=1, fan_out(service)=0 → I=0.00
        let graph = fixture_one_import();
        let dot = render(
            &graph,
            &stub_metrics(&arch_from_pairs(&[("domain", "service")])),
        );
        assert!(dot.contains(r#"label="service.py\nI=0.00""#));
    }

    #[test]
    fn test_render_unstable_node_should_have_exact_label() {
        // domain imports service → fan_in(domain)=0, fan_out(domain)=1 → I=1.00
        let graph = fixture_one_import();
        let dot = render(
            &graph,
            &stub_metrics(&arch_from_pairs(&[("domain", "service")])),
        );
        assert!(dot.contains(r#"label="domain.py\nI=1.00""#));
    }

    #[test]
    fn test_render_balanced_node_should_have_instability_zero_point_five() {
        let graph = fixture_balanced_node();
        let dot = render(
            &graph,
            &stub_metrics(&arch_from_pairs(&[
                ("balanced", "domain"),
                ("service", "balanced"),
            ])),
        );
        assert!(dot.contains(r#"label="balanced.py\nI=0.50""#));
    }

    #[test]
    fn test_render_hub_node_should_have_instability_zero_point_four() {
        let graph = fixture_hub_node();
        let dot = render(
            &graph,
            &stub_metrics(&arch_from_pairs(&[
                ("hub", "x"),
                ("hub", "y"),
                ("a", "hub"),
                ("b", "hub"),
                ("c", "hub"),
            ])),
        );
        assert!(dot.contains(r#"label="hub.py\nI=0.40""#));
    }

    #[test]
    fn test_render_mid_node_should_have_instability_zero_point_six_seven() {
        let graph = fixture_mid_node();
        let dot = render(
            &graph,
            &stub_metrics(&arch_from_pairs(&[
                ("mid", "x"),
                ("mid", "y"),
                ("consumer", "mid"),
            ])),
        );
        assert!(dot.contains(r#"label="mid.py\nI=0.67""#));
    }

    // ── edges ────────────────────────────────────────────────────────────────

    #[test]
    fn test_render_edge_should_point_from_importer_to_dependency() {
        // domain imports service → DOT arrow: domain_id -> service_id
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        let domain_idx = node_index_in_dot(&dot, "domain.py").expect("domain.py node");
        let service_idx = node_index_in_dot(&dot, "service.py").expect("service.py node");
        assert!(dot.contains(&format!("{domain_idx} -> {service_idx}")));
        assert!(!dot.contains(&format!("{service_idx} -> {domain_idx}")));
    }

    #[test]
    fn test_render_edge_should_show_failure_count_as_label() {
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.contains(r#"label="1""#));
    }

    #[test]
    fn test_render_edge_should_show_penwidth_for_single_import() {
        // count=1 → 1.0 + sqrt(1) = 2.00
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("penwidth=2.00"));
    }

    #[test]
    fn test_render_edge_should_accumulate_count_for_repeated_imports() {
        // 4 ModuleEdge entries for the same pair → count=4
        let graph = fixture_repeated_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.contains(r#"label="4""#));
    }

    #[test]
    fn test_render_edge_should_show_penwidth_for_repeated_imports() {
        // count=4 → 1.0 + sqrt(4) = 3.00
        let graph = fixture_repeated_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("penwidth=3.00"));
    }

    // ── test file exclusion ──────────────────────────────────────────────────

    #[test]
    fn test_render_should_exclude_test_file_node_from_output() {
        let graph = fixture_test_imports_source();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("test_domain.py"));
    }

    #[test]
    fn test_render_should_exclude_edge_involving_test_file() {
        let graph = fixture_test_imports_source();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("->"));
    }

    // ── isolated nodes ───────────────────────────────────────────────────────

    #[test]
    fn test_render_isolated_source_node_should_get_yellow_fill() {
        // orphan.py has no deps and nothing imports it
        use std::collections::BTreeSet;
        let domain_id = ModuleId(0);
        let service_id = ModuleId(1);
        let orphan_id = ModuleId(2);
        let mut modules = BTreeMap::new();
        for (id, name) in [
            (domain_id, "domain"),
            (service_id, "service"),
            (orphan_id, "orphan"),
        ] {
            modules.insert(
                id,
                Module::Source {
                    id,
                    name: QualifiedName(name.into()),
                    file_path: format!("{name}.py").into(),
                    object_ids: BTreeSet::new(),
                },
            );
        }
        let graph = ArchitectureGraph {
            modules,
            objects: BTreeMap::new(),
            dependencies: vec![],
            module_edges: vec![ModuleEdge {
                from: domain_id,
                to: service_id,
            }],
        };
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(
            dot.contains("fillcolor=yellow"),
            "orphan node should be yellow:\n{dot}"
        );
    }

    #[test]
    fn test_render_connected_node_should_not_get_yellow_fill() {
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("fillcolor=yellow"));
    }

    // ── cycle detection ──────────────────────────────────────────────────────

    #[test]
    fn test_render_cycle_nodes_should_get_lightcoral_fill() {
        let graph = fixture_cycle();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("lightcoral"));
    }

    #[test]
    fn test_render_non_cycle_node_should_not_get_lightcoral_fill() {
        let graph = fixture_one_import();
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("lightcoral"));
    }

    // ── edge cases ───────────────────────────────────────────────────────────

    #[test]
    fn test_render_with_empty_metrics_should_not_panic() {
        let graph = fixture_one_import();
        let empty = MetricsResult { nodes: vec![] };
        let dot = render(&graph, &empty);
        assert!(dot.contains("digraph coupling {"));
    }

    #[test]
    fn test_render_empty_graph_should_produce_valid_dot() {
        let graph = ArchitectureGraph {
            modules: BTreeMap::new(),
            objects: BTreeMap::new(),
            dependencies: vec![],
            module_edges: vec![],
        };
        let dot = render(&graph, &MetricsResult { nodes: vec![] });
        assert!(dot.starts_with("digraph coupling {"));
        assert!(dot.trim_end().ends_with('}'));
        assert_eq!(dot.matches("[shape=box").count(), 0);
        assert!(!dot.contains("->"));
    }
}
