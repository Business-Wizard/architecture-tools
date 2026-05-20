use comfy_table::{Cell, Color, Table, presets};

use crate::graph::coupling_graph::FileRole;
use crate::graph::graph_analysis::{ClusteringResult, RefactorHint, refactor_hints};
use crate::graph::metrics::{MetricsResult, NodeMetrics};
use crate::model::{BaselineResult, MutantResult, MutantStatus, VerifierStatus};

pub fn print_report(
    baseline: &BaselineResult,
    results: &[MutantResult],
    clustering: &ClusteringResult,
    metrics: &MetricsResult,
) {
    print_baseline_section(baseline);
    print_summary_section(results);
    print_centers_section(clustering);
    print_unexpected_section(clustering);
    print_refactor_section(clustering);
    print_metrics_section(metrics);
}

fn print_baseline_section(b: &BaselineResult) {
    println!("\n─── Baseline ───────────────────────────────────────────");
    print_verifier("basedpyright", &b.basedpyright);
    print_verifier("pytest", &b.pytest);
}

fn print_verifier(name: &str, status: &VerifierStatus) {
    match status {
        VerifierStatus::Pass => println!("  {name}: pass"),
        VerifierStatus::Fail(lines) => println!("  {name}: {} errors", lines.len()),
    }
}

fn print_summary_section(results: &[MutantResult]) {
    let breaks = results
        .iter()
        .filter(|r| r.status == MutantStatus::Breaks)
        .count();
    let survives = results
        .iter()
        .filter(|r| r.status == MutantStatus::Survives)
        .count();
    let invalid = results
        .iter()
        .filter(|r| r.status == MutantStatus::Invalid)
        .count();
    let timeout = results
        .iter()
        .filter(|r| r.status == MutantStatus::Timeout)
        .count();

    println!("\n─── Mutation Summary ────────────────────────────────────");
    println!("  Total:    {}", results.len());
    println!("  Breaks:   {breaks}");
    println!("  Survives: {survives}");
    println!("  Invalid:  {invalid}");
    if timeout > 0 {
        println!("  Timeout:  {timeout}");
    }
}

fn print_centers_section(clustering: &ClusteringResult) {
    use crate::model::OperatorKind;
    use std::collections::BTreeSet;

    if clustering.centers.is_empty() {
        return;
    }

    let centers = clustering.centers.iter().take(10).collect::<Vec<_>>();

    // Collect operators that appear in any center, in a stable order
    let active_ops: Vec<OperatorKind> = {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut ops = vec![];
        for c in &centers {
            let mut sorted: Vec<_> = c.operator_breakdown.keys().collect();
            sorted.sort_by_key(|op| op.short_name());
            for op in sorted {
                if seen.insert(op.to_string()) {
                    ops.push(op.clone());
                }
            }
        }
        ops
    };

    println!("\n─── Coupling Hotspots ───────────────────────────────────");
    let mut table = Table::new();
    table.load_preset(presets::UTF8_FULL);

    // Header row: File + one column per operator
    let mut header = vec![Cell::new("File")];
    for op in &active_ops {
        header.push(Cell::new(format!("{}\nsrc / test", op.short_name())));
    }
    table.set_header(header);

    for center in &centers {
        let mut row = vec![Cell::new(center.file.as_str())];
        for op in &active_ops {
            let cell = match center.operator_breakdown.get(op) {
                Some(b) if b.source > 0 || b.test > 0 => {
                    Cell::new(format!("{} / {}", b.source, b.test))
                }
                _ => Cell::new("—"),
            };
            row.push(cell);
        }
        table.add_row(row);
    }

    println!("{table}");
}

fn print_unexpected_section(clustering: &ClusteringResult) {
    if clustering.unexpected.is_empty() {
        return;
    }

    println!("\n─── Unexpected Cross-Package Coupling ───────────────────");
    let mut table = Table::new();
    table.load_preset(comfy_table::presets::UTF8_FULL);
    table.set_header(vec!["Mutant file", "Affected file", "Failures"]);

    for uc in clustering.unexpected.iter().take(15) {
        table.add_row(vec![
            Cell::new(uc.mutant_file.as_str()),
            Cell::new(uc.affected_file.as_str()),
            Cell::new(uc.failure_count.to_string()),
        ]);
    }

    println!("{table}");
}

fn print_refactor_section(clustering: &ClusteringResult) {
    let hints = refactor_hints(&clustering.centers);
    if hints.is_empty() {
        return;
    }

    println!("\n─── Refactor Areas ──────────────────────────────────────");
    for (file, hint) in &hints {
        let text = match hint {
            RefactorHint::ExtractTestFixture => {
                "only test files break — extract a shared test fixture or stub".to_string()
            }
            RefactorHint::BrittleCoupling { neighbor } => {
                format!("brittle coupling — consider decoupling from {neighbor}")
            }
            RefactorHint::StabilizeApiSurface => {
                "many files depend on this — stabilize and document the public API".to_string()
            }
            RefactorHint::NoRecommendation => {
                "no clear recommendation — inspect manually".to_string()
            }
        };
        println!("  • {file}: {text}");
    }
}

fn print_metrics_section(metrics: &MetricsResult) {
    let source_nodes: Vec<&NodeMetrics> = metrics
        .nodes
        .iter()
        .filter(|n| n.role == FileRole::Source)
        .collect();

    if source_nodes.is_empty() {
        return;
    }

    println!("\n─── Main Sequence ───────────────────────────────────────");

    let mut table = Table::new();
    table.load_preset(comfy_table::presets::UTF8_FULL);
    table.set_header(vec![
        "File", "Fan-in", "Fan-out", "Inst", "Abst", "Dist", "Status",
    ]);

    for node in source_nodes.iter().take(20) {
        let fmt_opt = |v: Option<f64>| v.map_or("—".to_string(), |x| format!("{x:.2}"));
        let status_cell = match (node.distance_failure, node.distance_warning) {
            (true, _) => Cell::new("FAIL").fg(Color::Red),
            (_, true) => Cell::new("warn").fg(Color::Yellow),
            _ => Cell::new(""),
        };
        table.add_row(vec![
            Cell::new(node.file.as_str()),
            Cell::new(node.fan_in.to_string()),
            Cell::new(node.fan_out.to_string()),
            Cell::new(fmt_opt(node.instability)),
            Cell::new(fmt_opt(node.abstractness)),
            Cell::new(fmt_opt(node.distance)),
            status_cell,
        ]);
    }
    println!("{table}");

    let warnings = source_nodes
        .iter()
        .filter(|n| n.distance_warning && !n.distance_failure)
        .count();
    let failures = source_nodes.iter().filter(|n| n.distance_failure).count();
    let on_sequence = source_nodes
        .iter()
        .filter(|n| n.distance.is_some_and(|d| d <= 0.3))
        .count();
    println!("  {on_sequence} on main sequence, {warnings} warnings, {failures} failures");
}
