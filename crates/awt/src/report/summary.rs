use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::graph::clustering::ClusteringResult;
use crate::model::{BaselineResult, MutantResult, MutantStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub mutants: Vec<MutantSummary>,
    pub baseline_passed: bool,
    pub total: usize,
    pub breaks: usize,
    pub survives: usize,
    pub invalid: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutantSummary {
    pub stable_id: String,
    pub file: String,
    pub symbol: String,
    pub operator: String,
    pub status: String,
    pub affected_files: Vec<String>,
    pub affected_packages: Vec<String>,
}

impl RunReport {
    pub fn build(
        baseline: &BaselineResult,
        results: &[MutantResult],
        _clustering: &ClusteringResult,
    ) -> Self {
        let mutants: Vec<MutantSummary> = results
            .iter()
            .map(|r| {
                let affected_files: Vec<String> = r
                    .affected_files()
                    .into_iter()
                    .map(std::string::ToString::to_string)
                    .collect();

                let affected_packages: Vec<String> = affected_files
                    .iter()
                    .filter_map(|f| f.split('/').next().map(String::from))
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                MutantSummary {
                    stable_id: r.candidate.id.to_string(),
                    file: r.candidate.file.to_string(),
                    symbol: r.candidate.symbol.clone(),
                    operator: r.candidate.operator.to_string(),
                    status: format!("{:?}", r.status).to_lowercase(),
                    affected_files,
                    affected_packages,
                }
            })
            .collect();

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

        RunReport {
            mutants,
            baseline_passed: baseline.all_pass(),
            total: results.len(),
            breaks,
            survives,
            invalid,
        }
    }
}

#[derive(Debug)]
pub struct DeltaRow {
    pub stable_id: String,
    pub before_affected: usize,
    pub after_affected: usize,
    pub delta: i64,
}

pub fn compute_delta(before: &RunReport, after: &RunReport) -> Vec<DeltaRow> {
    let before_map: HashMap<&str, &MutantSummary> = before
        .mutants
        .iter()
        .map(|m| (m.stable_id.as_str(), m))
        .collect();

    let mut rows: Vec<DeltaRow> = after
        .mutants
        .iter()
        .filter_map(|m| {
            let b = before_map.get(m.stable_id.as_str())?;
            let before_n = b.affected_files.len();
            let after_n = m.affected_files.len();
            if before_n == after_n {
                return None;
            }
            Some(DeltaRow {
                stable_id: m.stable_id.clone(),
                before_affected: before_n,
                after_affected: after_n,
                delta: i64::try_from(after_n).unwrap_or(i64::MAX)
                    - i64::try_from(before_n).unwrap_or(i64::MAX),
            })
        })
        .collect();

    rows.sort_by_key(|r| r.delta);
    rows
}

pub fn print_delta(rows: &[DeltaRow]) {
    if rows.is_empty() {
        println!("No changes in affected files between runs.");
        return;
    }

    println!("\n─── Before/After Delta ──────────────────────────────────");
    println!(
        "{:<60} {:>6} {:>6} {:>6}",
        "Mutant", "Before", "After", "Delta"
    );
    println!("{}", "─".repeat(78));

    for row in rows {
        let id = if row.stable_id.len() > 58 {
            format!("…{}", &row.stable_id[row.stable_id.len() - 57..])
        } else {
            row.stable_id.clone()
        };
        println!(
            "{:<60} {:>6} {:>6} {:>+6}",
            id, row.before_affected, row.after_affected, row.delta
        );
    }
}
