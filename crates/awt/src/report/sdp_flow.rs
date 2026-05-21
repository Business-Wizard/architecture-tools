use std::collections::HashMap;
use std::io;

use camino::Utf8Path;
use plotters::prelude::{
    BitMapBackend, IntoDrawingArea, PathElement, Polygon, RGBColor, Text, WHITE,
};
use plotters::style::text_anchor::{HPos, Pos, VPos};
use plotters::style::{Color, IntoFont};

use crate::graph::coupling_graph::{FileRole, GraphIndex};
use crate::graph::metrics::{
    Dependency, Depender, INSTABILITY_EPSILON, Instability, MetricsResult, violates_sdp,
};

const CHART_WIDTH: u32 = 900;
const ROW_HEIGHT: u32 = 28;
const MARGIN_LEFT: u32 = 180;
const MARGIN_RIGHT: u32 = 180;
const MARGIN_TOP: u32 = 60;
const MARGIN_BOTTOM: u32 = 50;
const PLOT_WIDTH: u32 = CHART_WIDTH - MARGIN_LEFT - MARGIN_RIGHT;
const ARROW_HEAD_LEN: i32 = 10;
const ARROW_HEAD_WIDTH: i32 = 5;
const MAX_ROWS: usize = 40;

const COLOUR_HEALTHY: RGBColor = RGBColor(30, 160, 70);
const COLOUR_VIOLATION: RGBColor = RGBColor(200, 50, 40);
const COLOUR_NEUTRAL: RGBColor = RGBColor(130, 130, 130);

/// Encapsulates axis orientation: stable (I=0) left, unstable (I=1) right.
/// Arrows flow left→right, from depender toward dependency (stable on left).
struct StabilityAxis;

impl StabilityAxis {
    fn x_pos(i: Instability) -> i32 {
        #[allow(clippy::cast_possible_truncation)]
        let offset = (i.as_f64() * f64::from(PLOT_WIDTH)) as i32;
        i32::try_from(MARGIN_LEFT).unwrap_or(0) + offset
    }
}

/// A directed dependency edge for the SDP chart. Depender depends on Dependency.
/// Constructed from coupling-graph edges via `from_coupling_edge` — the only site
/// where graph direction (src=mutation source) is translated to dependency direction.
struct SdpEdge {
    depender: Depender,
    dependency: Dependency,
    depender_label: String,
    dependency_label: String,
}

impl SdpEdge {
    /// Graph edge src→dst means "mutating src broke dst", so dst is the depender and src is the dependency.
    fn from_coupling_edge(
        graph_src: Dependency,
        graph_dst: Depender,
        depender_label: String,
        dependency_label: String,
    ) -> Self {
        Self {
            depender: graph_dst,
            dependency: graph_src,
            depender_label,
            dependency_label,
        }
    }

    fn is_violation(&self) -> bool {
        violates_sdp(self.dependency, self.depender)
    }

    fn colour(&self) -> RGBColor {
        let i_dep = self.dependency.0.as_f64();
        let i_per = self.depender.0.as_f64();
        if i_dep - i_per > INSTABILITY_EPSILON {
            COLOUR_VIOLATION
        } else if i_per - i_dep > INSTABILITY_EPSILON {
            COLOUR_HEALTHY
        } else {
            COLOUR_NEUTRAL
        }
    }
}

fn collect_edges(idx: &GraphIndex, metrics: &MetricsResult) -> Vec<SdpEdge> {
    let instability_map: HashMap<_, Instability> = metrics
        .nodes
        .iter()
        .map(|n| (&n.file, n.instability))
        .collect();

    let mut edges: Vec<SdpEdge> = idx
        .graph
        .edge_indices()
        .filter_map(|e| {
            let (src, dst) = idx.graph.edge_endpoints(e).unwrap();
            let src_node = &idx.graph[src];
            let dst_node = &idx.graph[dst];
            if src_node.role != FileRole::Source || dst_node.role != FileRole::Source {
                return None;
            }
            // Graph edge (src→dst) means "mutating src broke dst", so dst depends on src.
            let i_dependency = *instability_map.get(&src_node.path)?;
            let i_depender = *instability_map.get(&dst_node.path)?;
            let depender_label = dst_node
                .path
                .file_stem()
                .unwrap_or(dst_node.path.as_str())
                .to_owned();
            let dependency_label = src_node
                .path
                .file_stem()
                .unwrap_or(src_node.path.as_str())
                .to_owned();
            Some(SdpEdge::from_coupling_edge(
                Dependency(i_dependency),
                Depender(i_depender),
                depender_label,
                dependency_label,
            ))
        })
        .collect();

    // Violations first, then healthy edges
    edges.sort_by(|a, b| {
        b.is_violation()
            .cmp(&a.is_violation())
            .then(a.depender_label.cmp(&b.depender_label))
    });
    edges
}

fn draw_arrow(
    area: &plotters::drawing::DrawingArea<BitMapBackend<'_>, plotters::coord::Shift>,
    edge: &SdpEdge,
    row_y: i32,
    colour: RGBColor,
) -> io::Result<()> {
    let x_src = StabilityAxis::x_pos(edge.depender.0);
    let x_dst = StabilityAxis::x_pos(edge.dependency.0);

    // Shaft
    area.draw(&PathElement::new(
        vec![(x_src, row_y), (x_dst, row_y)],
        colour.stroke_width(2),
    ))
    .map_err(|e| io::Error::other(e.to_string()))?;

    // Arrowhead triangle pointing toward x_dst
    let head_sign = if x_dst >= x_src { 1 } else { -1 };
    let tip = (x_dst, row_y);
    let tail_a = (x_dst - head_sign * ARROW_HEAD_LEN, row_y - ARROW_HEAD_WIDTH);
    let tail_b = (x_dst - head_sign * ARROW_HEAD_LEN, row_y + ARROW_HEAD_WIDTH);
    area.draw(&Polygon::new(vec![tip, tail_a, tail_b], colour.filled()))
        .map_err(|e| io::Error::other(e.to_string()))?;

    let label_style = ("sans-serif", 11).into_font().color(&colour);

    // Labels sit just outside the arrow's start and end points.
    // Use outward-facing alignment so text never overlaps the shaft.
    let (src_align, dst_align) = if x_src <= x_dst {
        (HPos::Right, HPos::Left)
    } else {
        (HPos::Left, HPos::Right)
    };
    area.draw(&Text::new(
        edge.depender_label.clone(),
        (x_src - if x_src <= x_dst { 6 } else { -6 }, row_y - 6),
        label_style.clone().pos(Pos::new(src_align, VPos::Top)),
    ))
    .map_err(|e| io::Error::other(e.to_string()))?;

    area.draw(&Text::new(
        edge.dependency_label.clone(),
        (x_dst + if x_src <= x_dst { 6 } else { -6 }, row_y - 6),
        label_style.pos(Pos::new(dst_align, VPos::Top)),
    ))
    .map_err(|e| io::Error::other(e.to_string()))?;

    Ok(())
}

fn render_sdp_flow(edges: &[SdpEdge], path: &Utf8Path) -> io::Result<()> {
    let n_rows = edges.len().min(MAX_ROWS);
    let n_rows_u32 = u32::try_from(n_rows).unwrap_or(u32::MAX);
    let total_height = MARGIN_TOP + n_rows_u32 * ROW_HEIGHT + MARGIN_BOTTOM;

    let root = BitMapBackend::new(path.as_str(), (CHART_WIDTH, total_height)).into_drawing_area();
    root.fill(&WHITE)
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Title
    let title_style = ("sans-serif", 16)
        .into_font()
        .color(&plotters::style::BLACK);
    root.draw(&Text::new(
        "SDP Dependency Flow  (depender \u{2192} dependency)",
        (i32::try_from(CHART_WIDTH / 2).unwrap_or(0), 12),
        title_style.pos(Pos::new(HPos::Center, VPos::Top)),
    ))
    .map_err(|e| io::Error::other(e.to_string()))?;

    // X-axis line
    let axis_y = i32::try_from(total_height - MARGIN_BOTTOM).unwrap_or(0);
    let axis_x0 = i32::try_from(MARGIN_LEFT).unwrap_or(0);
    let axis_x1 = i32::try_from(CHART_WIDTH - MARGIN_RIGHT).unwrap_or(0);
    root.draw(&PathElement::new(
        vec![(axis_x0, axis_y), (axis_x1, axis_y)],
        plotters::style::BLACK.stroke_width(1),
    ))
    .map_err(|e| io::Error::other(e.to_string()))?;

    // X-axis ticks and labels
    let tick_style = ("sans-serif", 10)
        .into_font()
        .color(&plotters::style::BLACK);
    for &tick in &[0.0_f64, 0.25, 0.5, 0.75, 1.0] {
        let tx = StabilityAxis::x_pos(Instability::new(tick));
        root.draw(&PathElement::new(
            vec![(tx, axis_y), (tx, axis_y + 4)],
            plotters::style::BLACK.stroke_width(1),
        ))
        .map_err(|e| io::Error::other(e.to_string()))?;
        root.draw(&Text::new(
            format!("{tick:.2}"),
            (tx, axis_y + 6),
            tick_style.clone().pos(Pos::new(HPos::Center, VPos::Top)),
        ))
        .map_err(|e| io::Error::other(e.to_string()))?;
    }

    // Axis label
    let axis_label_style = ("sans-serif", 12)
        .into_font()
        .color(&plotters::style::BLACK);
    root.draw(&Text::new(
        "Instability (I)   stable \u{2192} unstable",
        (i32::try_from(CHART_WIDTH / 2).unwrap_or(0), axis_y + 20),
        axis_label_style.pos(Pos::new(HPos::Center, VPos::Top)),
    ))
    .map_err(|e| io::Error::other(e.to_string()))?;

    // Draw each arrow row
    for (row, edge) in edges.iter().take(MAX_ROWS).enumerate() {
        let row_i32 = i32::try_from(row).unwrap_or(i32::MAX);
        let row_y = i32::try_from(MARGIN_TOP).unwrap_or(0)
            + row_i32 * i32::try_from(ROW_HEIGHT).unwrap_or(28)
            + i32::try_from(ROW_HEIGHT / 2).unwrap_or(14);
        draw_arrow(&root, edge, row_y, edge.colour())?;
    }

    // Overflow note
    if edges.len() > MAX_ROWS {
        let note_style = ("sans-serif", 10)
            .into_font()
            .color(&plotters::style::BLACK);
        let max_rows_u32 = u32::try_from(MAX_ROWS).unwrap_or(u32::MAX);
        root.draw(&Text::new(
            format!(
                "Showing {} of {} edges (violations first)",
                MAX_ROWS,
                edges.len()
            ),
            (
                i32::try_from(CHART_WIDTH / 2).unwrap_or(0),
                i32::try_from(MARGIN_TOP).unwrap_or(0)
                    + i32::try_from(max_rows_u32 * ROW_HEIGHT).unwrap_or(0)
                    + 2,
            ),
            note_style.pos(Pos::new(HPos::Center, VPos::Top)),
        ))
        .map_err(|e| io::Error::other(e.to_string()))?;
    }

    root.present().map_err(|e| io::Error::other(e.to_string()))
}

/// # Errors
/// Returns `io::Error` if the PNG file cannot be written or rendered.
pub fn write_sdp_flow(
    idx: &GraphIndex,
    metrics: &MetricsResult,
    path: &Utf8Path,
) -> io::Result<()> {
    let edges = collect_edges(idx, metrics);
    render_sdp_flow(&edges, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::abstractness::AbstractnessMap;
    use crate::graph::coupling_graph::{CouplingEdge, CouplingGraph, CouplingNode};
    use crate::graph::metrics;
    use crate::model::{
        Candidate, CandidateKind, FailureCategory, FailureEvent, FailureScope, MutantId,
        MutantResult, MutantStatus, OperatorKind, VerifierKind,
    };
    use camino::Utf8PathBuf;
    use tempfile::NamedTempFile;

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

    fn stub_metrics(idx: &GraphIndex) -> MetricsResult {
        metrics::compute(
            idx,
            &AbstractnessMap {
                by_file: std::collections::HashMap::new(),
            },
        )
    }

    fn source_source_graph(src: &str, dst: &str) -> GraphIndex {
        GraphIndex::build(&[make_result(src, &[dst])], &[])
    }

    fn source_test_graph() -> GraphIndex {
        GraphIndex::build(
            &[make_result("src/domain.py", &["tests/test_domain.py"])],
            &[],
        )
    }

    fn make_large_graph(n: usize) -> (GraphIndex, MetricsResult) {
        let mut graph = CouplingGraph::new();
        let mut nodes = vec![];
        for i in 0..n {
            let path = Utf8PathBuf::from(format!("src/mod{i}.py"));
            let ni = graph.add_node(CouplingNode {
                path,
                role: FileRole::Source,
            });
            nodes.push(ni);
        }
        for i in 0..n - 1 {
            graph.add_edge(nodes[i], nodes[i + 1], CouplingEdge { failure_count: 1 });
        }
        let idx = GraphIndex { graph };
        let m = stub_metrics(&idx);
        (idx, m)
    }

    #[test]
    fn test_collect_edges_should_return_one_edge_for_single_dependency() {
        let idx = source_source_graph("src/a.py", "src/b.py");
        let m = stub_metrics(&idx);
        let edges = collect_edges(&idx, &m);
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn test_collect_edges_should_exclude_test_nodes() {
        let idx = source_test_graph();
        let m = stub_metrics(&idx);
        let edges = collect_edges(&idx, &m);
        assert_eq!(edges.len(), 0);
    }

    #[test]
    fn test_collect_edges_should_exclude_edges_with_missing_metrics() {
        let idx = source_source_graph("src/a.py", "src/b.py");
        let empty = MetricsResult { nodes: vec![] };
        let edges = collect_edges(&idx, &empty);
        assert_eq!(edges.len(), 0);
    }

    #[test]
    fn test_collect_edges_should_have_instability_values_in_unit_range() {
        // b→a: b has fan_out=1 (I=1.0), a has fan_in=1 (I=0.0).
        // In SDP terms: a depends on b → depender=a (I=1.0), dependency=b (I=0.0).
        let idx = source_source_graph("src/b.py", "src/a.py");
        let m = stub_metrics(&idx);
        let edges = collect_edges(&idx, &m);
        assert_eq!(edges.len(), 1);
        assert!((0.0..=1.0).contains(&edges[0].depender.0.as_f64()));
        assert!((0.0..=1.0).contains(&edges[0].dependency.0.as_f64()));
    }

    #[test]
    fn test_write_sdp_flow_with_valid_edges_should_produce_png_file() {
        let tmp = NamedTempFile::with_suffix(".png").unwrap();
        let path = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let idx = source_source_graph("src/domain.py", "src/service.py");
        let m = stub_metrics(&idx);
        let result = write_sdp_flow(&idx, &m, path.as_path());
        assert!(result.is_ok());
        assert!(std::fs::metadata(tmp.path()).unwrap().len() > 0);
    }

    #[test]
    fn test_write_sdp_flow_with_no_edges_should_produce_valid_empty_chart() {
        let tmp = NamedTempFile::with_suffix(".png").unwrap();
        let path = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let idx = GraphIndex::build(&[], &[]);
        let m = stub_metrics(&idx);
        let result = write_sdp_flow(&idx, &m, path.as_path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_write_sdp_flow_with_many_edges_should_cap_at_max_rows() {
        let tmp = NamedTempFile::with_suffix(".png").unwrap();
        let path = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let (idx, m) = make_large_graph(MAX_ROWS + 5);
        let result = write_sdp_flow(&idx, &m, path.as_path());
        assert!(result.is_ok());
    }
}
