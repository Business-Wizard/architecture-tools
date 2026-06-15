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

const CHART_WIDTH: u32 = 1800;
const ROW_HEIGHT: u32 = 56;
const MARGIN_LEFT: u32 = 360;
const MARGIN_RIGHT: u32 = 360;
const MARGIN_TOP: u32 = 120;
const MARGIN_BOTTOM: u32 = 100;
const PLOT_WIDTH: u32 = CHART_WIDTH - MARGIN_LEFT - MARGIN_RIGHT;
const ARROW_HEAD_LEN: i32 = 20;
const ARROW_HEAD_WIDTH: i32 = 10;
const MAX_ROWS: usize = 40;

const COLOUR_HEALTHY: RGBColor = RGBColor(30, 160, 70);
const COLOUR_VIOLATION: RGBColor = RGBColor(200, 50, 40);
const COLOUR_NEUTRAL: RGBColor = RGBColor(130, 130, 130);
const COLOUR_STEEP: RGBColor = RGBColor(220, 160, 0);
const STEEP_JUMP_THRESHOLD: f64 = 0.8;

#[derive(Debug, Clone, Copy, Default)]
pub enum EdgeOrder {
    /// Group by dependency module (most stable first), within group shortest jump first.
    #[default]
    ByDependencyThenJump,
    /// Violations first, then steep jumps, then healthy, alphabetically within tier.
    #[allow(dead_code)]
    BySeverity,
}

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
/// where graph direction (src=dependency source) is translated to display direction.
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

    fn is_steep(&self) -> bool {
        let delta = self.depender.0.as_f64() - self.dependency.0.as_f64();
        !self.is_violation() && delta >= STEEP_JUMP_THRESHOLD
    }

    fn jump_magnitude(&self) -> f64 {
        (self.depender.0.as_f64() - self.dependency.0.as_f64()).abs()
    }

    fn colour(&self) -> RGBColor {
        let i_dep = self.dependency.0.as_f64();
        let i_per = self.depender.0.as_f64();
        if i_dep - i_per > INSTABILITY_EPSILON {
            COLOUR_VIOLATION
        } else if i_per - i_dep >= STEEP_JUMP_THRESHOLD {
            COLOUR_STEEP
        } else if i_per - i_dep > INSTABILITY_EPSILON {
            COLOUR_HEALTHY
        } else {
            COLOUR_NEUTRAL
        }
    }
}

fn sort_edges(edges: &mut [SdpEdge], order: EdgeOrder) {
    match order {
        EdgeOrder::ByDependencyThenJump => edges.sort_by(|a, b| {
            a.dependency
                .0
                .as_f64()
                .partial_cmp(&b.dependency.0.as_f64())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a.jump_magnitude()
                        .partial_cmp(&b.jump_magnitude())
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then(a.depender_label.cmp(&b.depender_label))
        }),
        EdgeOrder::BySeverity => edges.sort_by(|a, b| {
            b.is_violation()
                .cmp(&a.is_violation())
                .then(b.is_steep().cmp(&a.is_steep()))
                .then(a.depender_label.cmp(&b.depender_label))
        }),
    }
}

fn collect_edges(idx: &GraphIndex, metrics: &MetricsResult, order: EdgeOrder) -> Vec<SdpEdge> {
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

    sort_edges(&mut edges, order);
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
        colour.stroke_width(4),
    ))
    .map_err(|e| io::Error::other(e.to_string()))?;

    // Arrowhead triangle pointing toward x_dst
    let head_sign = if x_dst >= x_src { 1 } else { -1 };
    let tip = (x_dst, row_y);
    let tail_a = (x_dst - head_sign * ARROW_HEAD_LEN, row_y - ARROW_HEAD_WIDTH);
    let tail_b = (x_dst - head_sign * ARROW_HEAD_LEN, row_y + ARROW_HEAD_WIDTH);
    area.draw(&Polygon::new(vec![tip, tail_a, tail_b], colour.filled()))
        .map_err(|e| io::Error::other(e.to_string()))?;

    let label_style = ("sans-serif", 22).into_font().color(&colour);

    // Labels sit just outside the arrow's start and end points.
    // Use outward-facing alignment so text never overlaps the shaft.
    let (src_align, dst_align) = if x_src <= x_dst {
        (HPos::Right, HPos::Left)
    } else {
        (HPos::Left, HPos::Right)
    };
    area.draw(&Text::new(
        edge.depender_label.clone(),
        (x_src - if x_src <= x_dst { 12 } else { -12 }, row_y - 12),
        label_style.clone().pos(Pos::new(src_align, VPos::Top)),
    ))
    .map_err(|e| io::Error::other(e.to_string()))?;

    area.draw(&Text::new(
        edge.dependency_label.clone(),
        (x_dst + if x_src <= x_dst { 12 } else { -12 }, row_y - 12),
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
    let title_style = ("sans-serif", 32)
        .into_font()
        .color(&plotters::style::BLACK);
    root.draw(&Text::new(
        "SDP Dependency Flow  (depender \u{2192} dependency)",
        (i32::try_from(CHART_WIDTH / 2).unwrap_or(0), 24),
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
    let tick_style = ("sans-serif", 20)
        .into_font()
        .color(&plotters::style::BLACK);
    for &tick in &[0.0_f64, 0.25, 0.5, 0.75, 1.0] {
        let tx = StabilityAxis::x_pos(Instability::new(tick));
        root.draw(&PathElement::new(
            vec![(tx, axis_y), (tx, axis_y + 8)],
            plotters::style::BLACK.stroke_width(2),
        ))
        .map_err(|e| io::Error::other(e.to_string()))?;
        root.draw(&Text::new(
            format!("{tick:.2}"),
            (tx, axis_y + 12),
            tick_style.clone().pos(Pos::new(HPos::Center, VPos::Top)),
        ))
        .map_err(|e| io::Error::other(e.to_string()))?;
    }

    // Axis label
    let axis_label_style = ("sans-serif", 24)
        .into_font()
        .color(&plotters::style::BLACK);
    root.draw(&Text::new(
        "Instability (I)   stable \u{2192} unstable",
        (i32::try_from(CHART_WIDTH / 2).unwrap_or(0), axis_y + 40),
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
        let note_style = ("sans-serif", 20)
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
    let edges = collect_edges(idx, metrics, EdgeOrder::default());
    render_sdp_flow(&edges, path)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use architecture_core::model::{
        ArchitectureGraph, Module, ModuleEdge, ModuleId, QualifiedName,
    };

    use super::*;
    use crate::graph::coupling_graph::{CouplingEdge, CouplingGraph, CouplingNode};
    use crate::graph::metrics;
    use camino::Utf8PathBuf;
    use tempfile::NamedTempFile;

    fn stub_metrics(graph: &ArchitectureGraph) -> MetricsResult {
        metrics::compute(graph)
    }

    fn arch_source_source_graph(src: &str, dst: &str) -> ArchitectureGraph {
        let src_id = ModuleId(0);
        let dst_id = ModuleId(1);
        let mut modules = BTreeMap::new();
        modules.insert(
            src_id,
            Module::Source {
                id: src_id,
                name: QualifiedName(src.to_owned()),
                file_path: src.into(),
                object_ids: BTreeSet::new(),
            },
        );
        modules.insert(
            dst_id,
            Module::Source {
                id: dst_id,
                name: QualifiedName(dst.to_owned()),
                file_path: dst.into(),
                object_ids: BTreeSet::new(),
            },
        );
        ArchitectureGraph {
            modules,
            objects: BTreeMap::new(),
            dependencies: vec![],
            module_edges: vec![ModuleEdge {
                from: src_id,
                to: dst_id,
            }],
        }
    }

    fn arch_source_test_graph() -> ArchitectureGraph {
        let src_id = ModuleId(0);
        let test_id = ModuleId(1);
        let mut modules = BTreeMap::new();
        modules.insert(
            src_id,
            Module::Source {
                id: src_id,
                name: QualifiedName("domain".to_owned()),
                file_path: "domain.py".into(),
                object_ids: BTreeSet::new(),
            },
        );
        modules.insert(
            test_id,
            Module::Test {
                id: test_id,
                name: QualifiedName("test_domain".to_owned()),
                file_path: "test_domain.py".into(),
                object_ids: BTreeSet::new(),
            },
        );
        ArchitectureGraph {
            modules,
            objects: BTreeMap::new(),
            dependencies: vec![],
            module_edges: vec![ModuleEdge {
                from: test_id,
                to: src_id,
            }],
        }
    }

    fn source_source_graph(src: &str, dst: &str) -> GraphIndex {
        let src_stem = std::path::Path::new(src)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let dst_stem = std::path::Path::new(dst)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let files = vec![Utf8PathBuf::from(src), Utf8PathBuf::from(dst)];
        let deps = vec![lang_core::ModuleDep {
            from: src_stem.into(),
            to: dst_stem.into(),
        }];
        GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer)
    }

    fn source_test_graph() -> GraphIndex {
        let files = vec![
            Utf8PathBuf::from("domain.py"),
            Utf8PathBuf::from("test_domain.py"),
        ];
        let deps = vec![lang_core::ModuleDep {
            from: "test_domain".into(),
            to: "domain".into(),
        }];
        GraphIndex::build_from_module_deps(&deps, &files, &py_analyzer::PythonAnalyzer)
    }

    fn make_large_graph(n: usize) -> (GraphIndex, ArchitectureGraph) {
        let mut graph = CouplingGraph::new();
        let mut coupling_nodes = vec![];
        for i in 0..n {
            let path = Utf8PathBuf::from(format!("src/mod{i}.py"));
            let ni = graph.add_node(CouplingNode {
                path,
                role: FileRole::Source,
            });
            coupling_nodes.push(ni);
        }
        for i in 0..n - 1 {
            graph.add_edge(
                coupling_nodes[i],
                coupling_nodes[i + 1],
                CouplingEdge { failure_count: 1 },
            );
        }
        let idx = GraphIndex { graph };

        let mut modules = BTreeMap::new();
        let mut module_edges = vec![];
        for i in 0..n {
            let mid = ModuleId(u32::try_from(i).expect("fits u32"));
            modules.insert(
                mid,
                Module::Source {
                    id: mid,
                    name: QualifiedName(format!("mod{i}")),
                    file_path: format!("src/mod{i}.py").into(),
                    object_ids: BTreeSet::new(),
                },
            );
        }
        for i in 0..n - 1 {
            module_edges.push(ModuleEdge {
                from: ModuleId(u32::try_from(i).expect("fits u32")),
                to: ModuleId(u32::try_from(i + 1).expect("fits u32")),
            });
        }
        let arch = ArchitectureGraph {
            modules,
            objects: BTreeMap::new(),
            dependencies: vec![],
            module_edges,
        };

        (idx, arch)
    }

    #[test]
    fn test_sdp_edge_colour_should_be_steep_for_large_healthy_jump() {
        let edge = SdpEdge::from_coupling_edge(
            Dependency(Instability::new(0.0)),
            Depender(Instability::new(1.0)),
            "depender".into(),
            "dependency".into(),
        );
        assert_eq!(edge.colour(), COLOUR_STEEP);
    }

    #[test]
    fn test_sdp_edge_colour_should_be_green_for_small_healthy_jump() {
        let edge = SdpEdge::from_coupling_edge(
            Dependency(Instability::new(0.3)),
            Depender(Instability::new(0.6)),
            "depender".into(),
            "dependency".into(),
        );
        assert_eq!(edge.colour(), COLOUR_HEALTHY);
    }

    #[test]
    fn test_collect_edges_should_return_one_edge_for_single_dependency() {
        let idx = source_source_graph("src/a.py", "src/b.py");
        let m = stub_metrics(&arch_source_source_graph("src/a.py", "src/b.py"));
        let edges = collect_edges(&idx, &m, EdgeOrder::default());
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn test_collect_edges_should_exclude_test_nodes() {
        let idx = source_test_graph();
        let m = stub_metrics(&arch_source_test_graph());
        let edges = collect_edges(&idx, &m, EdgeOrder::default());
        assert_eq!(edges.len(), 0);
    }

    #[test]
    fn test_collect_edges_should_exclude_edges_with_missing_metrics() {
        let idx = source_source_graph("src/a.py", "src/b.py");
        let empty = MetricsResult { nodes: vec![] };
        let edges = collect_edges(&idx, &empty, EdgeOrder::default());
        assert_eq!(edges.len(), 0);
    }

    #[test]
    fn test_collect_edges_should_have_instability_values_in_unit_range() {
        // b→a: b has fan_out=1 (I=1.0), a has fan_in=1 (I=0.0).
        // In SDP terms: a depends on b → depender=a (I=1.0), dependency=b (I=0.0).
        let idx = source_source_graph("src/b.py", "src/a.py");
        let m = stub_metrics(&arch_source_source_graph("src/b.py", "src/a.py"));
        let edges = collect_edges(&idx, &m, EdgeOrder::default());
        assert_eq!(edges.len(), 1);
        assert!((0.0..=1.0).contains(&edges[0].depender.0.as_f64()));
        assert!((0.0..=1.0).contains(&edges[0].dependency.0.as_f64()));
    }

    #[test]
    fn test_write_sdp_flow_with_valid_edges_should_produce_png_file() {
        let tmp = NamedTempFile::with_suffix(".png").unwrap();
        let path = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let idx = source_source_graph("src/domain.py", "src/service.py");
        let m = stub_metrics(&arch_source_source_graph("src/domain.py", "src/service.py"));
        let result = write_sdp_flow(&idx, &m, path.as_path());
        assert!(result.is_ok());
        assert!(std::fs::metadata(tmp.path()).unwrap().len() > 0);
    }

    #[test]
    fn test_write_sdp_flow_with_no_edges_should_produce_valid_empty_chart() {
        let tmp = NamedTempFile::with_suffix(".png").unwrap();
        let path = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let idx = GraphIndex::build_from_module_deps(&[], &[], &py_analyzer::PythonAnalyzer);
        let arch = ArchitectureGraph {
            modules: BTreeMap::new(),
            objects: BTreeMap::new(),
            dependencies: vec![],
            module_edges: vec![],
        };
        let m = stub_metrics(&arch);
        let result = write_sdp_flow(&idx, &m, path.as_path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_write_sdp_flow_with_many_edges_should_cap_at_max_rows() {
        let tmp = NamedTempFile::with_suffix(".png").unwrap();
        let path = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let (idx, arch) = make_large_graph(MAX_ROWS + 5);
        let m = stub_metrics(&arch);
        let result = write_sdp_flow(&idx, &m, path.as_path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_sort_edges_by_dependency_then_jump_should_group_by_dependency_instability() {
        // Two edges share dependency I=0.0; one has dependency I=0.5.
        // Expect: the I=0.0 pair appears before the I=0.5 edge.
        let mut edges = vec![
            SdpEdge::from_coupling_edge(
                Dependency(Instability::new(0.5)),
                Depender(Instability::new(0.9)),
                "late".into(),
                "mid".into(),
            ),
            SdpEdge::from_coupling_edge(
                Dependency(Instability::new(0.0)),
                Depender(Instability::new(0.8)),
                "b".into(),
                "stable".into(),
            ),
            SdpEdge::from_coupling_edge(
                Dependency(Instability::new(0.0)),
                Depender(Instability::new(0.5)),
                "a".into(),
                "stable".into(),
            ),
        ];
        sort_edges(&mut edges, EdgeOrder::ByDependencyThenJump);
        let dep_instabilities: Vec<f64> = edges.iter().map(|e| e.dependency.0.as_f64()).collect();
        assert_eq!(dep_instabilities, vec![0.0, 0.0, 0.5]);
    }

    #[test]
    fn test_sort_edges_by_severity_should_put_violations_first() {
        // One violation (depender.I < dependency.I), one healthy edge.
        let mut edges = vec![
            SdpEdge::from_coupling_edge(
                Dependency(Instability::new(0.3)),
                Depender(Instability::new(0.7)),
                "healthy_depender".into(),
                "healthy_dep".into(),
            ),
            SdpEdge::from_coupling_edge(
                Dependency(Instability::new(0.8)),
                Depender(Instability::new(0.2)),
                "violation_depender".into(),
                "violation_dep".into(),
            ),
        ];
        sort_edges(&mut edges, EdgeOrder::BySeverity);
        assert!(edges[0].is_violation());
    }
}
