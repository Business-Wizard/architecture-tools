/// Policy for handling external type references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalTypePolicy {
    Ignore,
    CountAbstractHints,
    CountAsConcrete,
    ErrorOnUnknown,
}

/// Policy for handling unknown type references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownTypePolicy {
    Ignore,
    CountAsConcrete,
    Error,
}

/// Granularity for counting dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyGranularity {
    UniqueTarget,
    Edge,
    Occurrence,
}

/// Policy for handling cycles in the abstraction graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CyclePolicy {
    BreakWithZero,
    BreakWithOne,
    Error,
}

/// Analysis policy controlling how metrics are computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisPolicy {
    pub external_type_policy: ExternalTypePolicy,
    pub unknown_type_policy: UnknownTypePolicy,
    pub module_dependency_granularity: DependencyGranularity,
    pub object_dependency_granularity: DependencyGranularity,
    pub cycle_policy: CyclePolicy,
}

impl Default for AnalysisPolicy {
    fn default() -> Self {
        Self {
            external_type_policy: ExternalTypePolicy::ErrorOnUnknown,
            unknown_type_policy: UnknownTypePolicy::Error,
            module_dependency_granularity: DependencyGranularity::UniqueTarget,
            object_dependency_granularity: DependencyGranularity::Occurrence,
            cycle_policy: CyclePolicy::BreakWithZero,
        }
    }
}
