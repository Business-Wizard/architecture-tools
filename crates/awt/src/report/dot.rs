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

    let abstractness_map: HashMap<_, f64> = metrics
        .nodes
        .iter()
        .map(|n| (&n.file, n.abstractness))
        .collect();

    let mut out = String::new();
    writeln!(out, "digraph coupling {{").unwrap();
    writeln!(out, "    rankdir=RL;").unwrap();

    for &n in &source_nodes {
        let node = &idx.graph[n];
        let i = instability_map.get(&node.path).copied().unwrap_or(0.0);
        let a = abstractness_map.get(&node.path).copied().unwrap_or(0.0);
        let label = format!(
            "{}\\nI={:.2}  A={:.2}",
            node.path.as_str().replace('"', "\\\""),
            i,
            a
        );
        let attrs = if cycles.contains(&n) {
            "shape=box style=filled fillcolor=lightcoral"
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
        writeln!(
            out,
            "    {} -> {} [label=\"{count}\" penwidth={pw:.2}];",
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
    use super::*;
    use crate::graph::abstractness::AbstractnessMap;
    use crate::graph::coupling_graph::GraphIndex;
    use crate::graph::metrics;
    use camino::Utf8PathBuf;

    fn stub_metrics(idx: &GraphIndex) -> MetricsResult {
        metrics::compute(
            idx,
            &AbstractnessMap {
                by_file: std::collections::HashMap::new(),
            },
        )
    }

    fn fixture_source_only() -> GraphIndex {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("domain.py"), b"import service\n").unwrap();
        std::fs::write(root.join("service.py"), b"").unwrap();
        let files = vec![
            Utf8PathBuf::from("domain.py"),
            Utf8PathBuf::from("service.py"),
        ];
        GraphIndex::build_from_source_imports(&files, root)
    }

    fn fixture_source_to_test() -> GraphIndex {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("domain.py"), b"").unwrap();
        std::fs::write(root.join("test_domain.py"), b"import domain\n").unwrap();
        let files = vec![
            Utf8PathBuf::from("domain.py"),
            Utf8PathBuf::from("test_domain.py"),
        ];
        GraphIndex::build_from_source_imports(&files, root)
    }

    #[test]
    fn test_render_should_produce_valid_dot_with_source_nodes_and_edges() {
        let idx = fixture_source_only();
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(dot.contains("digraph coupling {"));
        assert!(dot.contains("domain.py"));
        assert!(dot.contains("service.py"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_render_should_exclude_test_nodes_from_dot() {
        let idx = fixture_source_to_test();
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(!dot.contains("tests/test_domain.py"));
    }

    #[test]
    fn test_render_should_exclude_edges_to_test_nodes() {
        let idx = fixture_source_to_test();
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(!dot.contains("->"));
    }

    #[test]
    fn test_source_file_should_use_box_shape() {
        let idx = fixture_source_only();
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(dot.contains("shape=box"));
    }

    #[test]
    fn test_edge_should_have_penwidth() {
        let idx = fixture_source_only();
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(dot.contains("penwidth="));
    }

    #[test]
    fn test_cycle_nodes_should_get_lightcoral_fill() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.py"), b"import b\n").unwrap();
        std::fs::write(root.join("b.py"), b"import a\n").unwrap();
        let files = vec![Utf8PathBuf::from("a.py"), Utf8PathBuf::from("b.py")];
        let idx = GraphIndex::build_from_source_imports(&files, root);
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(dot.contains("lightcoral"));
    }

    #[test]
    fn test_penwidth_grows_with_count() {
        assert!(penwidth(9) > penwidth(1));
    }

    #[test]
    fn test_render_should_include_instability_label_on_nodes() {
        let idx = fixture_source_only();
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(dot.contains("I="));
    }

    #[test]
    fn test_render_should_show_instability_one_for_dependent_node() {
        // domain→service in coupling graph: service depends on domain.
        // service has no dependents → I=1.00 (unstable).
        let idx = fixture_source_only();
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(dot.contains("I=1.00"));
    }

    #[test]
    fn test_render_should_show_instability_zero_for_depended_on_node() {
        // domain→service in coupling graph: domain is depended on by service → I=0.00 (stable).
        let idx = fixture_source_only();
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(dot.contains("I=0.00"));
    }

    #[test]
    fn test_render_with_empty_metrics_should_not_panic() {
        let idx = fixture_source_only();
        let empty = MetricsResult { nodes: vec![] };
        let dot = render(&idx, &empty);
        assert!(dot.contains("digraph coupling {"));
    }

    #[test]
    fn test_render_should_include_abstractness_label_on_nodes() {
        let idx = fixture_source_only();
        let dot = render(&idx, &stub_metrics(&idx));
        assert!(dot.contains("A="));
    }
}
