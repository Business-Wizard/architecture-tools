#![allow(dead_code)]

pub mod layer_config;
pub mod rules;

use crate::config::FitnessConfig;
use crate::graph::coupling_graph::GraphIndex;
use crate::graph::metrics::MetricsResult;
use crate::model::{Severity, Violation};

pub struct FitnessReport {
    pub violations: Vec<Violation>,
}

impl FitnessReport {
    pub fn has_errors(&self) -> bool {
        self.violations
            .iter()
            .any(|v| v.severity == Severity::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.violations
            .iter()
            .any(|v| v.severity == Severity::Warning)
    }
}

pub fn evaluate_all(
    idx: &GraphIndex,
    metrics: &MetricsResult,
    config: &FitnessConfig,
) -> FitnessReport {
    let mut violations = vec![];
    violations.extend(rules::adp_no_cycles(idx, config));
    violations.extend(rules::sdp_stable_dependencies(idx, metrics, config));
    violations.extend(rules::main_sequence_distance(metrics, config));
    violations.extend(rules::dependency_rule(idx, config));
    violations.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(a.files.first().cmp(&b.files.first()))
    });
    FitnessReport { violations }
}
