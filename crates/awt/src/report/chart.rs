use std::io;

use camino::Utf8Path;
use plotters::prelude::{
    BitMapBackend, ChartBuilder, Circle, IntoDrawingArea, LineSeries, Polygon, RGBAColor, RGBColor,
    Text, WHITE,
};
use plotters::style::text_anchor::{HPos, Pos, VPos};
use plotters::style::{Color, IntoFont};

use crate::config::MainSequenceConfig;
use crate::graph::coupling_graph::FileRole;
use crate::graph::metrics::MetricsResult;

const CHART_WIDTH: u32 = 800;
const CHART_HEIGHT: u32 = 800;
/// Extra vertical space below the scatter plot for the legend table.
const LEGEND_HEIGHT: u32 = 250;
const MARGIN: u32 = 60;
const DOT_RADIUS: i32 = 6;
const LEGEND_ROW_PX: u32 = 16;
const LEGEND_COLS: usize = 2;
const LEGEND_PAD_X: i32 = 20;
const LEGEND_PAD_Y: i32 = 12;

const COLOUR_MAIN_SEQUENCE: RGBColor = RGBColor(50, 160, 80);
const COLOUR_ZONE_WATCH: RGBAColor = RGBAColor(255, 220, 0, 0.15);
const COLOUR_ZONE_ERROR: RGBAColor = RGBAColor(220, 50, 50, 0.15);
const COLOUR_DOT: RGBColor = RGBColor(60, 100, 200);

const AXIS_MIN: f64 = 0.0;
const AXIS_MAX: f64 = 1.0;

struct ChartPoint {
    abstractness: f64,
    instability: f64,
    label: String,
}

fn collect_points(metrics: &MetricsResult) -> Vec<ChartPoint> {
    metrics
        .nodes
        .iter()
        .filter(|n| n.role == FileRole::Source)
        .map(|n| {
            let label = n.file.file_stem().unwrap_or(n.file.as_str()).to_owned();
            ChartPoint {
                abstractness: n.abstractness,
                instability: n.instability,
                label,
            }
        })
        .collect()
}

fn zone_triangles(threshold: f64) -> [Vec<(f64, f64)>; 2] {
    // zone of pain (lower-left): a+i < 1-t  →  intersect a+i=1-t with unit square edges
    let lower = (AXIS_MAX - threshold).max(AXIS_MIN);
    let pain = vec![(AXIS_MIN, AXIS_MIN), (lower, AXIS_MIN), (AXIS_MIN, lower)];
    // zone of uselessness (upper-right): a+i > 1+t  →  intersect a+i=1+t with unit square edges
    // at a=1: i=t; at i=1: a=t  (both stay within [0,1])
    let useless = vec![
        (AXIS_MAX, threshold),
        (AXIS_MAX, AXIS_MAX),
        (threshold, AXIS_MAX),
    ];
    [pain, useless]
}

/// # Errors
/// Returns `io::Error` if the PNG file cannot be written or the chart cannot be rendered.
pub fn write_chart(
    metrics: &MetricsResult,
    config: &MainSequenceConfig,
    path: &Utf8Path,
) -> io::Result<()> {
    let points = collect_points(metrics);
    render_chart(&points, config, path)
}

fn render_chart(
    points: &[ChartPoint],
    config: &MainSequenceConfig,
    path: &Utf8Path,
) -> io::Result<()> {
    let total_height = CHART_HEIGHT + LEGEND_HEIGHT;
    let root = BitMapBackend::new(path.as_str(), (CHART_WIDTH, total_height)).into_drawing_area();
    root.fill(&WHITE)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let (scatter_area, legend_area) = root.split_vertically(CHART_HEIGHT);

    // ── Scatter plot ──────────────────────────────────────────────────────────

    let mut chart = ChartBuilder::on(&scatter_area)
        .caption("I vs A — Main Sequence", ("sans-serif", 22))
        .margin(MARGIN)
        .x_label_area_size(40)
        .y_label_area_size(40)
        .build_cartesian_2d(AXIS_MIN..AXIS_MAX, AXIS_MIN..AXIS_MAX)
        .map_err(|e| io::Error::other(e.to_string()))?;

    chart
        .configure_mesh()
        .x_desc("Abstractness (A)")
        .y_desc("Instability (I)")
        .draw()
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Watch zones first (larger, yellow), then error zones on top (red, corners)
    for triangle in zone_triangles(config.watch_threshold) {
        chart
            .draw_series(std::iter::once(Polygon::new(triangle, COLOUR_ZONE_WATCH)))
            .map_err(|e| io::Error::other(e.to_string()))?;
    }
    for triangle in zone_triangles(config.error_threshold) {
        chart
            .draw_series(std::iter::once(Polygon::new(triangle, COLOUR_ZONE_ERROR)))
            .map_err(|e| io::Error::other(e.to_string()))?;
    }

    // Main sequence diagonal: A + I = 1
    chart
        .draw_series(LineSeries::new(
            [(AXIS_MIN, AXIS_MAX), (AXIS_MAX, AXIS_MIN)],
            COLOUR_MAIN_SEQUENCE.stroke_width(2),
        ))
        .map_err(|e| io::Error::other(e.to_string()))?;

    let badge_style = ("sans-serif", 9).into_font().color(&WHITE);

    // Dot with number badge — legend maps numbers to filenames
    for (idx, pt) in points.iter().enumerate() {
        let n = idx + 1;
        chart
            .draw_series(std::iter::once(Circle::new(
                (pt.abstractness, pt.instability),
                DOT_RADIUS,
                COLOUR_DOT.filled(),
            )))
            .map_err(|e| io::Error::other(e.to_string()))?;
        chart
            .draw_series(std::iter::once(Text::new(
                n.to_string(),
                (pt.abstractness, pt.instability),
                badge_style
                    .clone()
                    .pos(Pos::new(HPos::Center, VPos::Center)),
            )))
            .map_err(|e| io::Error::other(e.to_string()))?;
    }

    let corner_style = ("sans-serif", 13)
        .into_font()
        .color(&plotters::style::BLACK);

    // Lower-left corner: Zone of Pain (stable + concrete = rigid)
    chart
        .draw_series(std::iter::once(Text::new(
            "Zone of Pain",
            (AXIS_MIN + 0.02, AXIS_MIN + 0.03),
            corner_style.clone().pos(Pos::new(HPos::Left, VPos::Bottom)),
        )))
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Upper-right corner: Zone of Uselessness (abstract + unstable = irrelevant)
    chart
        .draw_series(std::iter::once(Text::new(
            "Zone of Uselessness",
            (AXIS_MAX - 0.02, AXIS_MAX - 0.03),
            corner_style.pos(Pos::new(HPos::Right, VPos::Top)),
        )))
        .map_err(|e| io::Error::other(e.to_string()))?;

    // ── Legend table ──────────────────────────────────────────────────────────

    draw_legend(&legend_area, points)?;

    root.present().map_err(|e| io::Error::other(e.to_string()))
}

fn draw_legend<DB: plotters::prelude::DrawingBackend>(
    area: &plotters::drawing::DrawingArea<DB, plotters::coord::Shift>,
    points: &[ChartPoint],
) -> io::Result<()>
where
    DB::ErrorType: 'static,
{
    if points.is_empty() {
        return Ok(());
    }

    let entry_style = ("sans-serif", 12)
        .into_font()
        .color(&plotters::style::BLACK);
    let legend_cols_u32 = u32::try_from(LEGEND_COLS).unwrap_or(1);
    let col_width = i32::try_from(CHART_WIDTH / legend_cols_u32).unwrap_or(i32::MAX / 2);

    for (idx, pt) in points.iter().enumerate() {
        let n = idx + 1;
        let col = i32::try_from(idx % LEGEND_COLS).unwrap_or(0);
        let row = i32::try_from(idx / LEGEND_COLS).unwrap_or(0);

        let x = LEGEND_PAD_X + col * col_width;
        let y = LEGEND_PAD_Y + row * i32::try_from(LEGEND_ROW_PX).unwrap_or(16);

        // Coloured number circle
        area.draw(&Circle::new((x + 6, y + 6), 6, COLOUR_DOT.filled()))
            .map_err(|e| io::Error::other(e.to_string()))?;

        let text = format!("{n}  {}", pt.label);
        area.draw(&plotters::element::Text::new(
            text,
            (x + 18, y),
            entry_style.clone(),
        ))
        .map_err(|e| io::Error::other(e.to_string()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::coupling_graph::FileRole;
    use crate::graph::metrics::{MetricsResult, NodeMetrics};
    use camino::Utf8PathBuf;
    use tempfile::NamedTempFile;

    fn stub_node(abstractness: f64, instability: f64, role: FileRole) -> NodeMetrics {
        let distance = (abstractness + instability - 1.0).abs();
        NodeMetrics {
            file: Utf8PathBuf::from("src/x.py"),
            role,
            fan_in: 1,
            fan_out: 1,
            instability,
            abstractness,
            distance,
            distance_warning: distance > 0.3,
            distance_failure: distance > 0.5,
        }
    }

    fn stub_metrics(nodes: Vec<NodeMetrics>) -> MetricsResult {
        MetricsResult { nodes }
    }

    #[test]
    fn test_write_chart_with_valid_points_should_produce_png_file() {
        let tmp = NamedTempFile::with_suffix(".png").unwrap();
        let path = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let metrics = stub_metrics(vec![
            stub_node(0.5, 0.5, FileRole::Source),
            stub_node(0.0, 1.0, FileRole::Source),
            stub_node(1.0, 0.0, FileRole::Source),
        ]);
        let config = MainSequenceConfig::default();
        let result = write_chart(&metrics, &config, path.as_path());
        assert!(result.is_ok());
        assert!(std::fs::metadata(tmp.path()).unwrap().len() > 0);
    }

    #[test]
    fn test_write_chart_with_no_points_should_produce_valid_empty_chart() {
        let tmp = NamedTempFile::with_suffix(".png").unwrap();
        let path = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let metrics = stub_metrics(vec![]);
        let config = MainSequenceConfig::default();
        let result = write_chart(&metrics, &config, path.as_path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_collect_points_should_exclude_test_role_nodes() {
        let metrics = stub_metrics(vec![stub_node(0.5, 0.5, FileRole::Test)]);
        let actual = collect_points(&metrics);
        assert!(actual.is_empty());
    }
}
