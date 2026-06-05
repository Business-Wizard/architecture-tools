mod cycle;
mod god_module;
mod module_hub;

use std::collections::HashSet;

use py_analyzer::InspectResult;

use crate::model::{GraphSeverity, GraphViolation};

#[must_use]
pub fn run_all(result: &InspectResult) -> Vec<GraphViolation> {
    let filtered = filter_to_project_deps(result);
    let mut violations = vec![];
    violations.extend(cycle::check(&filtered));
    violations.extend(module_hub::check(&filtered));
    violations.extend(god_module::check(&filtered));
    violations.sort_by(|a, b| {
        let sev_ord = severity_ord(&b.severity).cmp(&severity_ord(&a.severity));
        sev_ord.then_with(|| a.message.cmp(&b.message))
    });
    violations
}

/// Keep only deps where both endpoints are project-owned modules.
/// A module is "project-owned" if it appears as a `from` in at least one dep
/// (i.e. we scanned it and extracted its imports).
fn filter_to_project_deps(result: &InspectResult) -> InspectResult {
    let project_modules: HashSet<&str> =
        result.module_deps.iter().map(|d| d.from.as_str()).collect();
    let module_deps = result
        .module_deps
        .iter()
        .filter(|d| project_modules.contains(d.to.as_str()))
        .cloned()
        .collect();
    InspectResult {
        module_deps,
        classes: result.classes.clone(),
    }
}

fn severity_ord(s: &GraphSeverity) -> u8 {
    match s {
        GraphSeverity::Error => 1,
        GraphSeverity::Warning => 0,
    }
}
