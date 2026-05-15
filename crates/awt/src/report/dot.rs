use std::fmt::Write as FmtWrite;
use std::io;

use camino::Utf8Path;

use crate::graph::coupling_graph::{FileRole, GraphIndex};

pub fn write_dot(idx: &GraphIndex, path: &Utf8Path) -> io::Result<()> {
    let dot = render(idx);
    std::fs::write(path.as_std_path(), dot)
}

fn render(idx: &GraphIndex) -> String {
    let mut out = String::new();
    writeln!(out, "digraph coupling {{").unwrap();
    writeln!(out, "    rankdir=LR;").unwrap();

    for n in idx.graph.node_indices() {
        let node = &idx.graph[n];
        let label = node.path.as_str().replace('"', "\\\"");
        let (shape, style) = if node.role == FileRole::Test {
            ("ellipse", " style=dashed")
        } else {
            ("box", "")
        };
        writeln!(
            out,
            "    {} [shape={shape}{style} label=\"{label}\"];",
            n.index()
        )
        .unwrap();
    }

    for e in idx.graph.edge_indices() {
        let (src, dst) = idx.graph.edge_endpoints(e).unwrap();
        let count = idx.graph[e].failure_count;
        writeln!(
            out,
            "    {} -> {} [label=\"{count}\"];",
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

    fn fixture_idx() -> GraphIndex {
        let candidate = Candidate {
            id: MutantId::new("src/domain.py", "fn", "add_required_parameter"),
            file: Utf8PathBuf::from("src/domain.py"),
            symbol: "fn".into(),
            kind: CandidateKind::Function,
            operator: OperatorKind::AddRequiredParameter,
            line: 1,
            byte_start: 0,
            byte_end: 1,
        };
        let result = MutantResult {
            candidate: candidate.clone(),
            status: MutantStatus::Breaks,
            local_failures: vec![],
            external_failures: vec![FailureEvent {
                mutant_id: candidate.id.clone(),
                command: VerifierKind::Pytest,
                file: Utf8PathBuf::from("tests/test_domain.py"),
                line: None,
                column: None,
                symbol: None,
                category: FailureCategory::TestAssertion,
                message: "fail".into(),
                scope: FailureScope::External,
            }],
        };
        GraphIndex::build(&[result])
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
}
