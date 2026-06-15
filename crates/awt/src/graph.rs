pub mod architecture_graph_builder;
pub mod coupling_graph;
pub mod metrics;
pub mod object_graph;
mod rules;
pub mod violations;

use lang_core::ModuleDep;
use violations::GraphViolation;

#[must_use]
pub fn analyze(deps: &[ModuleDep]) -> Vec<GraphViolation> {
    rules::run_all(deps)
}
