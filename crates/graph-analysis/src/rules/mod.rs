mod cycle;
mod god_module;
mod module_hub;

use py_analyzer::InspectResult;

use crate::model::{GraphSeverity, GraphViolation};

#[must_use]
pub fn run_all(result: &InspectResult) -> Vec<GraphViolation> {
    let mut violations = vec![];
    violations.extend(cycle::check(result));
    violations.extend(module_hub::check(result));
    violations.extend(god_module::check(result));
    violations.sort_by(|a, b| {
        let sev_ord = severity_ord(&b.severity).cmp(&severity_ord(&a.severity));
        sev_ord.then_with(|| a.message.cmp(&b.message))
    });
    violations
}

fn severity_ord(s: &GraphSeverity) -> u8 {
    match s {
        GraphSeverity::Error => 1,
        GraphSeverity::Warning => 0,
    }
}
