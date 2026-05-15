use comfy_table::{Cell, Table};

use crate::graph::clustering::{ClusteringResult, refactor_hints};
use crate::model::{BaselineResult, MutantResult, MutantStatus, VerifierStatus};

pub fn print_report(
    baseline: &BaselineResult,
    results: &[MutantResult],
    clustering: &ClusteringResult,
) {
    print_baseline_section(baseline);
    print_summary_section(results);
    print_centers_section(clustering);
    print_unexpected_section(clustering);
    print_refactor_section(clustering);
}

fn print_baseline_section(b: &BaselineResult) {
    println!("\n─── Baseline ───────────────────────────────────────────");
    print_verifier("ruff", &b.ruff);
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
    table.set_header(vec![
        "File",
        "Source code affected",
        "Test code affected",
        "Package",
    ]);

    for center in clustering.centers.iter().take(10) {
        table.add_row(vec![
            Cell::new(center.file.as_str()),
            Cell::new(center.affected_source_code.to_string()),
            Cell::new(center.affected_test_code.to_string()),
            Cell::new(&center.top_package),
        ]);
    }

    println!("{table}");
    println!("  Coupling components: {}", clustering.component_count);
}

fn print_unexpected_section(clustering: &ClusteringResult) {
    if clustering.unexpected.is_empty() {
        return;
    }

    println!("\n─── Unexpected Cross-Package Coupling ───────────────────");
    let mut table = Table::new();
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
    for hint in &hints {
        println!("  • {hint}");
    }
}
