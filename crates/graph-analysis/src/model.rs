use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphRuleId {
    LayerInversion,
    CrossAdapterCoupling,
    HighFanIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationKind {
    LayerInversion {
        from_module: String,
        to_class: String,
        from_layer: String,
        to_layer: String,
        fix_hint: String,
    },
    CrossAdapterCoupling {
        from_class: String,
        to_class: String,
        from_adapter: String,
        to_adapter: String,
        fix_hint: String,
    },
    HighFanIn {
        class: String,
        fan_in: usize,
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
