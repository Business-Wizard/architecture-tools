mod cross_adapter;
mod fan_in;
mod layer_inversion;

use py_analyzer::InspectResult;

use crate::config::GraphLayerConfig;
use crate::layer_resolver::LayerResolver;
use crate::model::{GraphSeverity, GraphViolation};

pub fn run_all(
    result: &InspectResult,
    resolver: &LayerResolver<'_>,
    config: &GraphLayerConfig,
) -> Vec<GraphViolation> {
    let mut violations = vec![];
    violations.extend(layer_inversion::check(result, resolver));
    violations.extend(cross_adapter::check(result, resolver));
    violations.extend(fan_in::check(result, config.fan_in_threshold));
    violations.sort_by(|a, b| {
        // Errors before warnings, then alphabetically by message.
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
