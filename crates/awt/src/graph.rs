pub mod architecture_graph_builder;
pub mod metrics;
mod rules;
pub mod violations;

use architecture_core::model::ArchitectureGraph;
use violations::GraphViolation;

#[must_use]
pub fn analyze(graph: &ArchitectureGraph) -> Vec<GraphViolation> {
    rules::run_all(graph)
}
