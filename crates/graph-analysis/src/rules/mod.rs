mod cycle;
mod god_module;
mod module_hub;

use lang_core::ModuleDep;

use crate::model::{GraphSeverity, GraphViolation};

#[must_use]
pub fn run_all(deps: &[ModuleDep]) -> Vec<GraphViolation> {
    let mut violations = vec![];
    violations.extend(cycle::check(deps));
    violations.extend(module_hub::check(deps));
    violations.extend(god_module::check(deps));
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
