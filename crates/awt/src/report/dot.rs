use std::collections::HashSet;
use std::fmt::Write as FmtWrite;
use std::io;

use camino::Utf8Path;
use petgraph::algo::tarjan_scc;
use petgraph::graph::NodeIndex;

use crate::graph::coupling_graph::{FileRole, GraphIndex};

pub fn write_dot(idx: &GraphIndex, path: &Utf8Path) -> io::Result<()> {
    let dot = render(idx);
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

fn render(idx: &GraphIndex) -> String {
    let cycles = cycle_nodes(idx);
    let mut out = String::new();
    writeln!(out, "digraph coupling {{").unwrap();
    writeln!(out, "    rankdir=LR;").unwrap();

    for n in idx.graph.node_indices() {
        let node = &idx.graph[n];
        let label = node.path.as_str().replace('"', "\\\"");
        let in_cycle = cycles.contains(&n);

        let attrs = match (&node.role, in_cycle) {
            (FileRole::Test, false) => "shape=ellipse style=dashed".to_string(),
            (FileRole::Test, true) => {
                "shape=ellipse style=\"dashed,filled\" fillcolor=lightcoral".to_string()
            }
            (FileRole::Source, false) => "shape=box".to_string(),
            (FileRole::Source, true) => "shape=box style=filled fillcolor=lightcoral".to_string(),
        };

        writeln!(out, "    {} [{attrs} label=\"{label}\"];", n.index()).unwrap();
    }

    for e in idx.graph.edge_indices() {
        let (src, dst) = idx.graph.edge_endpoints(e).unwrap();
        let count = idx.graph[e].failure_count;
        let pw = penwidth(count);
        writeln!(
            out,
            "    {} -> {} [label=\"{count}\" penwidth={pw:.2}];",
            src.index(),
            dst.index()
        )
        .unwrap();
    }

    writeln!(out, "}}").unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::coupling_graph::GraphIndex;
    use crate::model::{
        Candidate, CandidateKind, FailureCategory, FailureEvent, FailureScope, MutantId,
        MutantResult, MutantStatus, OperatorKind, VerifierKind,
    };
    use camino::Utf8PathBuf;

    fn make_result(src: &str, affected: &[&str]) -> MutantResult {
        let candidate = Candidate {
            id: MutantId::new(src, "fn", "add_required_parameter"),
            file: Utf8PathBuf::from(src),
            symbol: "fn".into(),
            kind: CandidateKind::Function,
            operator: OperatorKind::AddRequiredParameter,
            line: 1,
            byte_start: 0,
            byte_end: 1,
        };
        let external_failures = affected
            .iter()
            .map(|f| FailureEvent {
                mutant_id: candidate.id.clone(),
                command: VerifierKind::Pytest,
                file: Utf8PathBuf::from(*f),
                line: None,
                column: None,
                symbol: None,
                category: FailureCategory::TestAssertion,
                message: "fail".into(),
                scope: FailureScope::External,
            })
            .collect();
        MutantResult {
            candidate,
            status: MutantStatus::Breaks,
            local_failures: vec![],
            external_failures,
        }
    }

    fn fixture_idx() -> GraphIndex {
        GraphIndex::build(&[make_result("src/domain.py", &["tests/test_domain.py"])])
    }

    #[test]
    fn test_render_should_produce_valid_dot_with_nodes_and_edges() {
        let dot = render(&fixture_idx());
        assert!(dot.contains("digraph coupling {"));
        assert!(dot.contains("src/domain.py"));
        assert!(dot.contains("tests/test_domain.py"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_test_file_should_use_dashed_ellipse() {
        let dot = render(&fixture_idx());
        assert!(dot.contains("style=dashed"));
        assert!(dot.contains("shape=ellipse"));
    }

    #[test]
    fn test_source_file_should_use_box_shape() {
        let dot = render(&fixture_idx());
        assert!(dot.contains("shape=box"));
    }

    #[test]
    fn test_edge_should_have_penwidth() {
        let dot = render(&fixture_idx());
        assert!(dot.contains("penwidth="));
    }

    #[test]
    fn test_cycle_nodes_should_get_lightcoral_fill() {
        // A→B and B→A creates a cycle
        let results = vec![
            make_result("src/a.py", &["src/b.py"]),
            make_result("src/b.py", &["src/a.py"]),
        ];
        let idx = GraphIndex::build(&results);
        let dot = render(&idx);
        assert!(dot.contains("lightcoral"));
    }

    #[test]
    fn test_penwidth_grows_with_count() {
        assert!(penwidth(9) > penwidth(1));
    }
}
