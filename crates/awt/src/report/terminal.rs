use comfy_table::{Cell, Table};

use crate::graph::clustering::{ClusteringResult, RefactorHint, refactor_hints};
use crate::graph::coupling_graph::FileRole;
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
    if clustering.centers.is_empty() {
        return;
    }

    println!("\n─── Top Centers of Gravity ──────────────────────────────");
    let mut table = Table::new();
    table.load_preset(comfy_table::presets::UTF8_FULL);
    table.set_header(vec!["File", "Source code affected", "Test code affected"]);

    for center in clustering.centers.iter().take(10) {
        table.add_row(vec![
            Cell::new(center.file.as_str()),
            Cell::new(center.affected_source_code.to_string()),
            Cell::new(center.affected_test_code.to_string()),
        ]);
    }

    println!("{table}");
    let component_msg = if clustering.component_count == 1 {
        "all files are tightly interconnected".to_string()
    } else {
        format!("{} separate coupling groups", clustering.component_count)
    };
    println!("  Coupling: {component_msg}");
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
    table.set_header(vec!["File", "Fan-in", "Fan-out", "I", "A", "D", "Status"]);

    for node in source_nodes.iter().take(20) {
        let fmt_opt = |v: Option<f64>| v.map_or("—".to_string(), |x| format!("{x:.2}"));
        let status = match (node.distance_failure, node.distance_warning) {
            (true, _) => "\x1b[31mFAIL\x1b[0m",
            (_, true) => "\x1b[33mwarn\x1b[0m",
            _ => "",
        };
        table.add_row(vec![
            Cell::new(node.file.as_str()),
            Cell::new(node.fan_in.to_string()),
            Cell::new(node.fan_out.to_string()),
            Cell::new(fmt_opt(node.instability)),
            Cell::new(fmt_opt(node.abstractness)),
            Cell::new(fmt_opt(node.distance)),
            Cell::new(status),
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
