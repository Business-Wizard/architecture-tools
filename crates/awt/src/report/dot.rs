use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::io;

use camino::Utf8Path;
use petgraph::algo::tarjan_scc;
use petgraph::graph::NodeIndex;

use crate::graph::coupling_graph::{FileRole, GraphIndex};
use crate::graph::metrics::MetricsResult;

pub fn write_dot(idx: &GraphIndex, metrics: &MetricsResult, path: &Utf8Path) -> io::Result<()> {
    let dot = render(idx, metrics);
    std::fs::write(path.as_std_path(), dot)
}

fn cycle_nodes(idx: &GraphIndex) -> HashSet<NodeIndex> {
    tarjan_scc(&idx.graph)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .flatten()
        .collect()
}

/// Returns the set of module name prefixes (directory stems) for all files in a coupling cycle.
/// Used to cross-highlight object nodes whose containing module participates in a file-level cycle.
pub fn cycle_module_names(idx: &GraphIndex) -> HashSet<String> {
    cycle_nodes(idx)
        .into_iter()
        .filter_map(|n| {
            let path = &idx.graph[n].path;
            // Use the immediate parent directory as the module stem.
            path.parent().map(|p| p.as_str().replace('/', "."))
        })
        .collect()
}

fn penwidth(count: usize) -> f32 {
    // 1.0 at count=1, grows with sqrt to avoid runaway thickness
    // failure_count is always small in practice; cap to avoid any precision concern
    let capped = u32::try_from(count).unwrap_or(u32::MAX);
    1.0_f32 + f32::from(u16::try_from(capped).unwrap_or(u16::MAX)).sqrt()
}

fn render(idx: &GraphIndex, metrics: &MetricsResult) -> String {
    let cycles = cycle_nodes(idx);
    let source_nodes: HashSet<NodeIndex> = idx
        .graph
        .node_indices()
        .filter(|&n| idx.graph[n].role == FileRole::Source)
        .collect();

    let instability_map: HashMap<_, f64> = metrics
        .nodes
        .iter()
        .map(|n| (&n.file, n.instability.as_f64()))
        .collect();

    let mut out = String::new();
    writeln!(out, "digraph coupling {{").unwrap();
    writeln!(out, "    rankdir=RL;").unwrap();

    for &n in &source_nodes {
        let node = &idx.graph[n];
        let i = instability_map.get(&node.path).copied().unwrap_or(0.0);
        let label = format!("{}\\nI={:.2}", node.path.as_str().replace('"', "\\\""), i);
        let is_isolated = idx.graph.edges(n).count() == 0
            && idx
                .graph
                .edges_directed(n, petgraph::Direction::Incoming)
                .count()
                == 0;
        let attrs = if cycles.contains(&n) {
            "shape=box style=filled fillcolor=lightcoral"
        } else if is_isolated {
            "shape=box style=filled fillcolor=yellow"
        } else {
            "shape=box"
        };
        writeln!(out, "    {} [{attrs} label=\"{label}\"];", n.index()).unwrap();
    }

    for e in idx.graph.edge_indices() {
        let (src, dst) = idx.graph.edge_endpoints(e).unwrap();
        if !source_nodes.contains(&src) || !source_nodes.contains(&dst) {
            continue;
        }
        let count = idx.graph[e].failure_count;
        let pw = penwidth(count);
        let cycle_edge = cycles.contains(&src) && cycles.contains(&dst);
        let color_attr = if cycle_edge { " color=crimson" } else { "" };
        writeln!(
            out,
            "    {} -> {} [label=\"{count}\" penwidth={pw:.2}{color_attr}];",
            dst.index(),
            src.index()
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
    use crate::graph::coupling_graph::GraphIndex;
    use crate::graph::metrics;
    use camino::Utf8PathBuf;
    use lang_core::ModuleDep;

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

    fn node_index_in_dot(dot: &str, filename: &str) -> Option<usize> {
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
    /// service: `fan_in=1`, `fan_out=0` → I=0.00 (stable)
    /// domain:  `fan_in=0`, `fan_out=1` → I=1.00 (unstable)
    fn fixture_one_import() -> GraphIndex {
        let files = vec![
            Utf8PathBuf::from("domain.py"),
            Utf8PathBuf::from("service.py"),
        ];
        let deps = vec![ModuleDep {
            from: "domain".into(),
            to: "service".into(),
        }];
        GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer)
    }

    /// `test_domain.py` imports domain.py — test node should be excluded from output
    fn fixture_test_imports_source() -> GraphIndex {
        let files = vec![
            Utf8PathBuf::from("domain.py"),
            Utf8PathBuf::from("test_domain.py"),
        ];
        let deps = vec![ModuleDep {
            from: "test_domain".into(),
            to: "domain".into(),
        }];
        GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer)
    }

    /// a.py and b.py mutually import each other → cycle
    fn fixture_cycle() -> GraphIndex {
        let files = vec![Utf8PathBuf::from("a.py"), Utf8PathBuf::from("b.py")];
        let deps = vec![
            ModuleDep {
                from: "a".into(),
                to: "b".into(),
            },
            ModuleDep {
                from: "b".into(),
                to: "a".into(),
            },
        ];
        GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer)
    }

    /// balanced.py imports domain.py; service.py imports balanced.py
    /// balanced: `fan_in=1`, `fan_out=1` → I=0.50
    fn fixture_balanced_node() -> GraphIndex {
        let files = vec![
            Utf8PathBuf::from("domain.py"),
            Utf8PathBuf::from("balanced.py"),
            Utf8PathBuf::from("service.py"),
        ];
        let deps = vec![
            ModuleDep {
                from: "balanced".into(),
                to: "domain".into(),
            },
            ModuleDep {
                from: "service".into(),
                to: "balanced".into(),
            },
        ];
        GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer)
    }

    /// hub.py imported by a, b, c; hub imports x, y
    /// hub: `fan_in=3`, `fan_out=2` → I=2/5=0.40
    fn fixture_hub_node() -> GraphIndex {
        let files = vec![
            Utf8PathBuf::from("x.py"),
            Utf8PathBuf::from("y.py"),
            Utf8PathBuf::from("hub.py"),
            Utf8PathBuf::from("a.py"),
            Utf8PathBuf::from("b.py"),
            Utf8PathBuf::from("c.py"),
        ];
        let deps = vec![
            ModuleDep {
                from: "hub".into(),
                to: "x".into(),
            },
            ModuleDep {
                from: "hub".into(),
                to: "y".into(),
            },
            ModuleDep {
                from: "a".into(),
                to: "hub".into(),
            },
            ModuleDep {
                from: "b".into(),
                to: "hub".into(),
            },
            ModuleDep {
                from: "c".into(),
                to: "hub".into(),
            },
        ];
        GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer)
    }

    /// mid.py imported by consumer; mid imports x, y
    /// mid: `fan_in=1`, `fan_out=2` → I=2/3≈0.67
    fn fixture_mid_node() -> GraphIndex {
        let files = vec![
            Utf8PathBuf::from("x.py"),
            Utf8PathBuf::from("y.py"),
            Utf8PathBuf::from("mid.py"),
            Utf8PathBuf::from("consumer.py"),
        ];
        let deps = vec![
            ModuleDep {
                from: "mid".into(),
                to: "x".into(),
            },
            ModuleDep {
                from: "mid".into(),
                to: "y".into(),
            },
            ModuleDep {
                from: "consumer".into(),
                to: "mid".into(),
            },
        ];
        GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer)
    }

    /// consumer imports hub 4 times (e.g. different named imports resolving to same file)
    /// edge count=4 → penwidth = 1.0 + sqrt(4) = 3.00
    fn fixture_repeated_import() -> GraphIndex {
        let files = vec![
            Utf8PathBuf::from("hub.py"),
            Utf8PathBuf::from("consumer.py"),
        ];
        let deps = vec![
            ModuleDep {
                from: "consumer".into(),
                to: "hub".into(),
            },
            ModuleDep {
                from: "consumer".into(),
                to: "hub".into(),
            },
            ModuleDep {
                from: "consumer".into(),
                to: "hub".into(),
            },
            ModuleDep {
                from: "consumer".into(),
                to: "hub".into(),
            },
        ];
        GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer)
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
        let idx = fixture_cycle();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("color=crimson"));
    }

    #[test]
    fn test_non_cycle_edges_should_not_get_crimson_color() {
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("color=crimson"));
    }

    #[test]
    fn test_penwidth_grows_with_count() {
        assert!(penwidth(9) > penwidth(1));
    }

    // ── structure ────────────────────────────────────────────────────────────

    #[test]
    fn test_render_should_open_with_digraph_coupling() {
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.starts_with("digraph coupling {"));
    }

    #[test]
    fn test_render_should_have_rankdir_rl() {
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("rankdir=RL;"));
    }

    #[test]
    fn test_render_should_include_exactly_two_source_nodes() {
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert_eq!(dot.matches("[shape=box").count(), 2);
    }

    // ── node labels ──────────────────────────────────────────────────────────

    #[test]
    fn test_render_should_use_box_shape_for_source_nodes() {
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("shape=box"));
    }

    #[test]
    fn test_render_stable_node_should_have_exact_label() {
        // domain imports service → fan_in(service)=1, fan_out(service)=0 → I=0.00
        let idx = fixture_one_import();
        let dot = render(
            &idx,
            &stub_metrics(&arch_from_pairs(&[("domain", "service")])),
        );
        assert!(dot.contains(r#"label="service.py\nI=0.00""#));
    }

    #[test]
    fn test_render_unstable_node_should_have_exact_label() {
        // domain imports service → fan_in(domain)=0, fan_out(domain)=1 → I=1.00
        let idx = fixture_one_import();
        let dot = render(
            &idx,
            &stub_metrics(&arch_from_pairs(&[("domain", "service")])),
        );
        assert!(dot.contains(r#"label="domain.py\nI=1.00""#));
    }

    #[test]
    fn test_render_balanced_node_should_have_instability_zero_point_five() {
        // balanced imports domain; service imports balanced → fan_in=1, fan_out=1 → I=0.50
        let idx = fixture_balanced_node();
        let dot = render(
            &idx,
            &stub_metrics(&arch_from_pairs(&[
                ("balanced", "domain"),
                ("service", "balanced"),
            ])),
        );
        assert!(dot.contains(r#"label="balanced.py\nI=0.50""#));
    }

    #[test]
    fn test_render_hub_node_should_have_instability_zero_point_four() {
        // hub imports x,y; a,b,c import hub → fan_in=3, fan_out=2 → I=2/5=0.40
        let idx = fixture_hub_node();
        let dot = render(
            &idx,
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
        // mid imports x,y; consumer imports mid → fan_in=1, fan_out=2 → I=2/3≈0.67
        let idx = fixture_mid_node();
        let dot = render(
            &idx,
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
        // domain imports service → DOT arrow: domain_idx -> service_idx (not the reverse)
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        let domain_idx = node_index_in_dot(&dot, "domain.py").expect("domain.py node");
        let service_idx = node_index_in_dot(&dot, "service.py").expect("service.py node");
        assert!(dot.contains(&format!("{domain_idx} -> {service_idx}")));
        assert!(!dot.contains(&format!("{service_idx} -> {domain_idx}")));
    }

    #[test]
    fn test_render_edge_should_show_failure_count_as_label() {
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.contains(r#"label="1""#));
    }

    #[test]
    fn test_render_edge_should_show_penwidth_for_single_import() {
        // count=1 → 1.0 + sqrt(1) = 2.00
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("penwidth=2.00"));
    }

    #[test]
    fn test_render_edge_should_accumulate_count_for_repeated_imports() {
        // 4 ModuleDep entries resolving to the same edge → count=4
        let idx = fixture_repeated_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.contains(r#"label="4""#));
    }

    #[test]
    fn test_render_edge_should_show_penwidth_for_repeated_imports() {
        // count=4 → 1.0 + sqrt(4) = 3.00
        let idx = fixture_repeated_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("penwidth=3.00"));
    }

    // ── test file exclusion ──────────────────────────────────────────────────

    #[test]
    fn test_render_should_exclude_test_file_node_from_output() {
        let idx = fixture_test_imports_source();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("test_domain.py"));
    }

    #[test]
    fn test_render_should_exclude_edge_involving_test_file() {
        let idx = fixture_test_imports_source();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("->"));
    }

    // ── isolated nodes ───────────────────────────────────────────────────────

    #[test]
    fn test_render_isolated_source_node_should_get_yellow_fill() {
        // orphan.py has no deps and nothing imports it.
        let files = vec![
            Utf8PathBuf::from("domain.py"),
            Utf8PathBuf::from("service.py"),
            Utf8PathBuf::from("orphan.py"),
        ];
        let deps = vec![ModuleDep {
            from: "domain".into(),
            to: "service".into(),
        }];
        let idx = GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer);
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(
            dot.contains("fillcolor=yellow"),
            "orphan node should be yellow:\n{dot}"
        );
    }

    #[test]
    fn test_render_connected_node_should_not_get_yellow_fill() {
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("fillcolor=yellow"));
    }

    // ── cycle detection ──────────────────────────────────────────────────────

    #[test]
    fn test_render_cycle_nodes_should_get_lightcoral_fill() {
        let idx = fixture_cycle();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.contains("lightcoral"));
    }

    #[test]
    fn test_render_non_cycle_node_should_not_get_lightcoral_fill() {
        let idx = fixture_one_import();
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(!dot.contains("lightcoral"));
    }

    // ── edge cases ───────────────────────────────────────────────────────────

    #[test]
    fn test_render_with_empty_metrics_should_not_panic() {
        let idx = fixture_one_import();
        let empty = MetricsResult { nodes: vec![] };
        let dot = render(&idx, &empty);
        assert!(dot.contains("digraph coupling {"));
    }

    #[test]
    fn test_render_empty_graph_should_produce_valid_dot() {
        let idx = GraphIndex::build_from_module_deps(&[], &[], &py_analyzer::PythonAnalyzer);
        let dot = render(&idx, &MetricsResult { nodes: vec![] });
        assert!(dot.starts_with("digraph coupling {"));
        assert!(dot.trim_end().ends_with('}'));
        assert_eq!(dot.matches("[shape=box").count(), 0);
        assert!(!dot.contains("->"));
    }
}
