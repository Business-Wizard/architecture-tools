use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphRuleId {
    CyclicDependency,
    ModuleHub,
    GodModule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationKind {
    CyclicDependency {
        modules: Vec<String>,
    },
    ModuleHub {
        module: String,
        fan_in: usize,
        threshold: usize,
    },
    GodModule {
        module: String,
        fan_out: usize,
        threshold: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphViolation {
    pub rule: GraphRuleId,
    pub severity: GraphSeverity,
    pub message: String,
    pub kind: ViolationKind,
}
