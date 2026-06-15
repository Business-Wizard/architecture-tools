use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::error::{ArchitectureError, Result};
use crate::metrics::{AbstractnessBasis, AbstractnessMetric, InstabilityMetric, RatioMetric};
use crate::model::{
    ArchitectureGraph, DependencyEdge, ExternalAbstractionHint, ModuleId, ObjectId, ObjectKind,
    OperationAbstraction, TypeRef,
};
use crate::policy::{
    AnalysisPolicy, CyclePolicy, DependencyGranularity, ExternalTypePolicy, UnknownTypePolicy,
};

/// Validate the integrity of an architecture graph.
///
/// # Errors
///
/// Returns `ObjectReferencesUnknownModule` if an object's module does not exist,
/// `ModuleReferencesUnknownObject` if a module lists an unknown object,
/// `InvalidModelInvariant` if an object's `module_id` mismatches the containing module,
/// `DependencyReferencesUnknownSource` if an edge source is missing, or
/// `DependencyReferencesUnknownTarget` if an internal edge target is missing.
pub fn validate_graph(graph: &ArchitectureGraph) -> Result<()> {
    for obj in graph.objects.values() {
        if !graph.modules.contains_key(&obj.module_id) {
            return Err(ArchitectureError::ObjectReferencesUnknownModule {
                object_id: obj.id,
                module_id: obj.module_id,
            });
        }
    }

    for module in graph.modules.values() {
        for &obj_id in module.object_ids() {
            if !graph.objects.contains_key(&obj_id) {
                return Err(ArchitectureError::ModuleReferencesUnknownObject {
                    module_id: module.id(),
                    object_id: obj_id,
                });
            }
        }
    }

    for module in graph.modules.values() {
        for &obj_id in module.object_ids() {
            let obj = &graph.objects[&obj_id];
            if obj.module_id != module.id() {
                return Err(ArchitectureError::InvalidModelInvariant {
                    message: format!(
                        "object {obj_id} has module_id {} but is in module {}",
                        obj.module_id,
                        module.id()
                    ),
                });
            }
        }
    }

    for edge in &graph.dependencies {
        if !graph.objects.contains_key(&edge.source) {
            return Err(ArchitectureError::DependencyReferencesUnknownSource(
                edge.source,
            ));
        }
    }

    for edge in &graph.dependencies {
        if let TypeRef::Internal(target_id) = edge.target
            && !graph.objects.contains_key(&target_id)
        {
            return Err(ArchitectureError::DependencyReferencesUnknownTarget {
                source: edge.source,
                target: edge.target.clone(),
            });
        }
    }

    Ok(())
}

/// Trait for computing instability metrics.
pub trait InstabilityStrategy {
    /// Compute instability for a single object.
    ///
    /// # Errors
    ///
    /// Returns an error if the graph is malformed or metric construction fails.
    fn object_instability(
        &self,
        graph: &ArchitectureGraph,
        object_id: ObjectId,
        policy: &AnalysisPolicy,
    ) -> Result<InstabilityMetric>;

    /// Compute instability for a module.
    ///
    /// # Errors
    ///
    /// Returns an error if the graph is malformed or metric construction fails.
    fn module_instability(
        &self,
        graph: &ArchitectureGraph,
        module_id: ModuleId,
        policy: &AnalysisPolicy,
    ) -> Result<InstabilityMetric>;
}

/// Instability strategy based on dependency graph structure.
#[derive(Debug, Clone)]
pub struct DependencyGraphInstability;

#[allow(clippy::cast_precision_loss)]
fn count_weight(
    unique_targets: &BTreeSet<ObjectId>,
    edges: &[&DependencyEdge],
    granularity: DependencyGranularity,
) -> f64 {
    match granularity {
        DependencyGranularity::UniqueTarget => unique_targets.len() as f64,
        DependencyGranularity::Edge => edges.len() as f64,
        DependencyGranularity::Occurrence => {
            edges.iter().map(|e| e.occurrence_count.max(1) as f64).sum()
        }
    }
}

fn instability_metric(ce: f64, ca: f64) -> Result<InstabilityMetric> {
    let ratio = RatioMetric::new(ce, ce + ca)?;
    Ok(InstabilityMetric {
        outgoing_dependency_weight: ce,
        incoming_dependent_weight: ca,
        ratio,
    })
}

impl InstabilityStrategy for DependencyGraphInstability {
    fn object_instability(
        &self,
        graph: &ArchitectureGraph,
        object_id: ObjectId,
        policy: &AnalysisPolicy,
    ) -> Result<InstabilityMetric> {
        let mut out_targets: BTreeSet<ObjectId> = BTreeSet::new();
        let mut out_edges: Vec<&DependencyEdge> = Vec::new();
        let mut in_targets: BTreeSet<ObjectId> = BTreeSet::new();
        let mut in_edges: Vec<&DependencyEdge> = Vec::new();

        for edge in &graph.dependencies {
            if edge.source == object_id
                && let TypeRef::Internal(target_id) = edge.target
                && target_id != object_id
            {
                out_targets.insert(target_id);
                out_edges.push(edge);
            }
            if let TypeRef::Internal(target_id) = edge.target
                && target_id == object_id
                && edge.source != object_id
            {
                in_targets.insert(edge.source);
                in_edges.push(edge);
            }
        }

        let g = policy.object_dependency_granularity;
        instability_metric(
            count_weight(&out_targets, &out_edges, g),
            count_weight(&in_targets, &in_edges, g),
        )
    }

    fn module_instability(
        &self,
        graph: &ArchitectureGraph,
        module_id: ModuleId,
        policy: &AnalysisPolicy,
    ) -> Result<InstabilityMetric> {
        let module = &graph.modules[&module_id];
        let module_objects: HashSet<ObjectId> = module.object_ids().iter().copied().collect();

        let mut out_targets: BTreeSet<ObjectId> = BTreeSet::new();
        let mut out_edges: Vec<&DependencyEdge> = Vec::new();
        let mut in_targets: BTreeSet<ObjectId> = BTreeSet::new();
        let mut in_edges: Vec<&DependencyEdge> = Vec::new();

        for edge in &graph.dependencies {
            if module_objects.contains(&edge.source)
                && let TypeRef::Internal(target_id) = edge.target
                && graph.objects[&target_id].module_id != module_id
            {
                out_targets.insert(target_id);
                out_edges.push(edge);
            }
            if let TypeRef::Internal(target_id) = edge.target
                && module_objects.contains(&target_id)
                && !module_objects.contains(&edge.source)
            {
                in_targets.insert(edge.source);
                in_edges.push(edge);
            }
        }

        let g = policy.module_dependency_granularity;
        instability_metric(
            count_weight(&out_targets, &out_edges, g),
            count_weight(&in_targets, &in_edges, g),
        )
    }
}

/// Returns the intrinsic abstractness of an object kind.
///
/// `TraitLike` variants score `1.0`; all others score `0.0`.
#[must_use]
pub fn intrinsic_abstractness(kind: &ObjectKind) -> f64 {
    match kind {
        ObjectKind::TraitLike(_) => 1.0,
        _ => 0.0,
    }
}

/// Returns `Some((abstract_score, 1.0))` if the `TypeRef` participates in abstractness
/// under the given policy, or `None` if it should be excluded from the count.
///
/// # Errors
///
/// Returns an error when the policy demands strict treatment of unknown types.
fn score_type_ref(
    ty: &TypeRef,
    policy: AnalysisPolicy,
    recurse: impl Fn(ObjectId) -> Result<f64>,
) -> Result<Option<f64>> {
    match ty {
        TypeRef::Internal(dep_id) => Ok(Some(recurse(*dep_id)?)),
        TypeRef::External {
            abstraction_hint, ..
        } => match policy.external_type_policy {
            ExternalTypePolicy::Ignore => Ok(None),
            ExternalTypePolicy::CountAsConcrete => Ok(Some(0.0)),
            ExternalTypePolicy::CountAbstractHints => match abstraction_hint {
                ExternalAbstractionHint::Abstract => Ok(Some(1.0)),
                ExternalAbstractionHint::Concrete => Ok(Some(0.0)),
                ExternalAbstractionHint::Unknown => Ok(None),
            },
            ExternalTypePolicy::ErrorOnUnknown => match abstraction_hint {
                ExternalAbstractionHint::Abstract => Ok(Some(1.0)),
                ExternalAbstractionHint::Concrete => Ok(Some(0.0)),
                ExternalAbstractionHint::Unknown => {
                    Err(ArchitectureError::UnknownExternalAbstraction { ty: ty.clone() })
                }
            },
        },
        TypeRef::Primitive { .. } => Ok(None),
        TypeRef::Unknown { raw } => match policy.unknown_type_policy {
            UnknownTypePolicy::Ignore => Ok(None),
            UnknownTypePolicy::CountAsConcrete => Ok(Some(0.0)),
            UnknownTypePolicy::Error => Err(ArchitectureError::UnknownType { raw: raw.clone() }),
        },
    }
}

/// Returns `Some(score)` if the operation abstraction contributes to the metric,
/// or `None` if it should be excluded.
///
/// # Errors
///
/// Returns an error when the policy demands strict treatment of unknown abstractions.
fn score_operation_abstraction(
    abstraction: OperationAbstraction,
    object_id: ObjectId,
    operation_name: &str,
    policy: AnalysisPolicy,
) -> Result<Option<f64>> {
    match abstraction {
        OperationAbstraction::Abstract => Ok(Some(1.0)),
        OperationAbstraction::Concrete => Ok(Some(0.0)),
        OperationAbstraction::Unknown => match policy.unknown_type_policy {
            UnknownTypePolicy::Ignore => Ok(None),
            UnknownTypePolicy::CountAsConcrete => Ok(Some(0.0)),
            UnknownTypePolicy::Error => Err(ArchitectureError::UnknownOperationAbstraction {
                object_id,
                operation_name: operation_name.to_owned(),
            }),
        },
    }
}

/// Abstractness strategy using compositional dependency-weighted scoring.
#[derive(Debug)]
pub struct CompositionalAbstractness {
    memo: RefCell<HashMap<ObjectId, f64>>,
    visiting: RefCell<HashSet<ObjectId>>,
}

impl CompositionalAbstractness {
    /// Create a new analyzer with empty memo and cycle-detection state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            memo: RefCell::new(HashMap::new()),
            visiting: RefCell::new(HashSet::new()),
        }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn compute_object(
        &self,
        object_id: ObjectId,
        graph: &ArchitectureGraph,
        policy: &AnalysisPolicy,
    ) -> Result<f64> {
        if let Some(&cached) = self.memo.borrow().get(&object_id) {
            return Ok(cached);
        }

        if self.visiting.borrow().contains(&object_id) {
            return match policy.cycle_policy {
                CyclePolicy::BreakWithZero => Ok(0.0),
                CyclePolicy::BreakWithOne => Ok(1.0),
                CyclePolicy::Error => Err(ArchitectureError::AbstractnessCycle { object_id }),
            };
        }
        self.visiting.borrow_mut().insert(object_id);

        let obj = &graph.objects[&object_id];
        let mut abstracts: f64 = 0.0;
        let mut count: f64 = 0.0;

        for constructor in &obj.constructors {
            for param in &constructor.parameters {
                if let Some(s) = score_type_ref(&param.ty, *policy, |dep_id| {
                    self.compute_object(dep_id, graph, policy)
                })? {
                    abstracts += s;
                    count += 1.0;
                }
            }
        }

        for operation in &obj.operations {
            if let Some(s) = score_operation_abstraction(
                operation.abstraction,
                object_id,
                &operation.name,
                *policy,
            )? {
                abstracts += s;
                count += 1.0;
            }
            for param in &operation.parameters {
                if let Some(s) = score_type_ref(&param.ty, *policy, |dep_id| {
                    self.compute_object(dep_id, graph, policy)
                })? {
                    abstracts += s;
                    count += 1.0;
                }
            }
        }

        #[allow(clippy::float_cmp)]
        let score = if count == 0.0 {
            intrinsic_abstractness(&obj.kind)
        } else {
            abstracts / count
        };

        self.visiting.borrow_mut().remove(&object_id);
        self.memo.borrow_mut().insert(object_id, score);
        Ok(score)
    }
}

impl Default for CompositionalAbstractness {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for computing abstractness metrics.
pub trait AbstractnessStrategy {
    /// Compute abstractness for a single object.
    ///
    /// # Errors
    ///
    /// Returns an error if the graph is malformed or policy demands strict type resolution.
    fn object_abstractness(
        &self,
        graph: &ArchitectureGraph,
        object_id: ObjectId,
        policy: &AnalysisPolicy,
    ) -> Result<AbstractnessMetric>;

    /// Compute abstractness for a module.
    ///
    /// # Errors
    ///
    /// Returns an error if the graph is malformed or policy demands strict type resolution.
    fn module_abstractness(
        &self,
        graph: &ArchitectureGraph,
        module_id: ModuleId,
        policy: &AnalysisPolicy,
    ) -> Result<AbstractnessMetric>;
}

impl AbstractnessStrategy for CompositionalAbstractness {
    fn object_abstractness(
        &self,
        graph: &ArchitectureGraph,
        object_id: ObjectId,
        policy: &AnalysisPolicy,
    ) -> Result<AbstractnessMetric> {
        let score = self.compute_object(object_id, graph, policy)?;
        let ratio = RatioMetric::new(score, 1.0)?;
        Ok(AbstractnessMetric {
            abstract_weight: score,
            total_weight: 1.0,
            ratio,
            basis: AbstractnessBasis::CompositionalConstruction,
        })
    }

    fn module_abstractness(
        &self,
        graph: &ArchitectureGraph,
        module_id: ModuleId,
        policy: &AnalysisPolicy,
    ) -> Result<AbstractnessMetric> {
        let module = &graph.modules[&module_id];
        let mut sum: f64 = 0.0;
        #[allow(clippy::cast_precision_loss)]
        let total_weight = module.object_ids().len() as f64;

        for &obj_id in module.object_ids() {
            sum += self.compute_object(obj_id, graph, policy)?;
        }

        let ratio = RatioMetric::new(sum, total_weight)?;
        Ok(AbstractnessMetric {
            abstract_weight: sum,
            total_weight,
            ratio,
            basis: AbstractnessBasis::ModuleObjects,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    #![allow(clippy::wildcard_imports)]

    use super::*;
    use crate::metrics::Score;
    use crate::model::{
        CodeObject, Constructor, Module, Operation, OperationKind, Parameter, QualifiedName,
        TraitLikeKind, TypeKind,
    };
    use crate::policy::{
        CyclePolicy, DependencyGranularity, ExternalTypePolicy, UnknownTypePolicy,
    };

    fn make_object(id: ObjectId, module_id: ModuleId, name: &str, kind: ObjectKind) -> CodeObject {
        CodeObject {
            id,
            module_id,
            name: QualifiedName(name.to_string()),
            kind,
            constructors: Vec::new(),
            operations: Vec::new(),
        }
    }

    fn make_source_module(id: ModuleId, name: &str, object_ids: Vec<ObjectId>) -> Module {
        Module::Source {
            id,
            name: QualifiedName(name.to_string()),
            file_path: "src/stub.py".into(),
            object_ids: object_ids.into_iter().collect(),
        }
    }

    fn make_test_module(id: ModuleId, name: &str, object_ids: Vec<ObjectId>) -> Module {
        Module::Test {
            id,
            name: QualifiedName(name.to_string()),
            file_path: "tests/stub_test.py".into(),
            object_ids: object_ids.into_iter().collect(),
        }
    }

    fn make_graph(
        modules: Vec<Module>,
        objects: Vec<CodeObject>,
        dependencies: Vec<DependencyEdge>,
    ) -> ArchitectureGraph {
        ArchitectureGraph {
            modules: modules.into_iter().map(|m| (m.id(), m)).collect(),
            objects: objects.into_iter().map(|o| (o.id, o)).collect(),
            dependencies,
            module_edges: vec![],
        }
    }

    fn strict_policy() -> AnalysisPolicy {
        AnalysisPolicy::default()
    }

    fn ignore_policy() -> AnalysisPolicy {
        AnalysisPolicy {
            external_type_policy: ExternalTypePolicy::Ignore,
            unknown_type_policy: UnknownTypePolicy::Ignore,
            ..Default::default()
        }
    }

    #[test]
    fn test_score_valid_values_should_succeed() {
        assert!(Score::new(0.0).is_ok());
        assert!(Score::new(0.5).is_ok());
        assert!(Score::new(1.0).is_ok());
    }

    #[test]
    fn test_score_out_of_range_should_fail() {
        assert!(Score::new(-0.1).is_err());
        assert!(Score::new(1.1).is_err());
    }

    #[test]
    fn test_ratio_metric_zero_denominator_should_return_none_score() {
        let metric = RatioMetric::new(5.0, 0.0).unwrap();
        assert_eq!(metric.score, None);
    }

    #[test]
    fn test_object_instability_unique_target_should_count_unique_targets() {
        let mid = ModuleId(1);
        let oid_a = ObjectId(1);
        let oid_b = ObjectId(2);
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid_a, oid_b])],
            vec![
                make_object(oid_a, mid, "A", ObjectKind::Constant),
                make_object(oid_b, mid, "B", ObjectKind::Constant),
            ],
            vec![
                DependencyEdge {
                    source: oid_a,
                    target: TypeRef::Internal(oid_b),
                    kind: crate::model::DependencyKind::Calls,
                    occurrence_count: 2,
                },
                DependencyEdge {
                    source: oid_a,
                    target: TypeRef::Internal(oid_b),
                    kind: crate::model::DependencyKind::Calls,
                    occurrence_count: 1,
                },
            ],
        );
        let policy = AnalysisPolicy {
            object_dependency_granularity: DependencyGranularity::UniqueTarget,
            ..Default::default()
        };
        let metric = DependencyGraphInstability
            .object_instability(&graph, oid_a, &policy)
            .unwrap();
        assert_eq!(metric.outgoing_dependency_weight, 1.0);
    }

    #[test]
    fn test_object_instability_occurrence_should_weight_by_count() {
        let mid = ModuleId(1);
        let oid_a = ObjectId(1);
        let oid_b = ObjectId(2);
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid_a, oid_b])],
            vec![
                make_object(oid_a, mid, "A", ObjectKind::Constant),
                make_object(oid_b, mid, "B", ObjectKind::Constant),
            ],
            vec![DependencyEdge {
                source: oid_a,
                target: TypeRef::Internal(oid_b),
                kind: crate::model::DependencyKind::Calls,
                occurrence_count: 3,
            }],
        );
        let policy = AnalysisPolicy {
            object_dependency_granularity: DependencyGranularity::Occurrence,
            ..Default::default()
        };
        let metric = DependencyGraphInstability
            .object_instability(&graph, oid_a, &policy)
            .unwrap();
        assert_eq!(metric.outgoing_dependency_weight, 3.0);
    }

    #[test]
    fn test_module_instability_same_module_edges_should_be_ignored() {
        let mid = ModuleId(1);
        let oid_a = ObjectId(1);
        let oid_b = ObjectId(2);
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid_a, oid_b])],
            vec![
                make_object(oid_a, mid, "A", ObjectKind::Constant),
                make_object(oid_b, mid, "B", ObjectKind::Constant),
            ],
            vec![DependencyEdge {
                source: oid_a,
                target: TypeRef::Internal(oid_b),
                kind: crate::model::DependencyKind::Calls,
                occurrence_count: 1,
            }],
        );
        let metric = DependencyGraphInstability
            .module_instability(&graph, mid, &AnalysisPolicy::default())
            .unwrap();
        assert_eq!(metric.outgoing_dependency_weight, 0.0);
    }

    #[test]
    fn test_module_instability_cross_module_edges_should_be_counted() {
        let mid1 = ModuleId(1);
        let mid2 = ModuleId(2);
        let oid_a = ObjectId(1);
        let oid_b = ObjectId(2);
        let graph = make_graph(
            vec![
                make_source_module(mid1, "m1", vec![oid_a]),
                make_source_module(mid2, "m2", vec![oid_b]),
            ],
            vec![
                make_object(oid_a, mid1, "A", ObjectKind::Constant),
                make_object(oid_b, mid2, "B", ObjectKind::Constant),
            ],
            vec![DependencyEdge {
                source: oid_a,
                target: TypeRef::Internal(oid_b),
                kind: crate::model::DependencyKind::Calls,
                occurrence_count: 1,
            }],
        );
        let metric = DependencyGraphInstability
            .module_instability(&graph, mid1, &AnalysisPolicy::default())
            .unwrap();
        assert_eq!(metric.outgoing_dependency_weight, 1.0);
    }

    #[test]
    fn test_intrinsic_abstractness_trait_like_should_return_one() {
        assert_eq!(
            intrinsic_abstractness(&ObjectKind::TraitLike(TraitLikeKind::RustTrait)),
            1.0
        );
    }

    #[test]
    fn test_intrinsic_abstractness_struct_should_return_zero() {
        assert_eq!(
            intrinsic_abstractness(&ObjectKind::Type(TypeKind::Struct)),
            0.0
        );
    }

    #[test]
    fn test_compositional_abstractness_mixed_deps_and_ops_should_blend_scores() {
        let mid = ModuleId(1);
        let oid_a = ObjectId(1);
        let oid_b = ObjectId(2);
        let mut obj_b = make_object(oid_b, mid, "B", ObjectKind::Type(TypeKind::Struct));
        obj_b.constructors = vec![Constructor {
            name: Some("new".to_string()),
            parameters: vec![Parameter {
                name: "a".to_string(),
                ty: TypeRef::Internal(oid_a),
            }],
        }];
        obj_b.operations = vec![
            Operation {
                name: "concrete_method".to_string(),
                kind: OperationKind::InstanceMethod,
                abstraction: OperationAbstraction::Concrete,
                parameters: Vec::new(),
                return_type: None,
            },
            Operation {
                name: "abstract_method".to_string(),
                kind: OperationKind::InstanceMethod,
                abstraction: OperationAbstraction::Abstract,
                parameters: Vec::new(),
                return_type: None,
            },
        ];
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid_a, oid_b])],
            vec![
                make_object(
                    oid_a,
                    mid,
                    "A",
                    ObjectKind::TraitLike(TraitLikeKind::RustTrait),
                ),
                obj_b,
            ],
            Vec::new(),
        );
        let metric = CompositionalAbstractness::new()
            .object_abstractness(&graph, oid_b, &AnalysisPolicy::default())
            .unwrap();
        let score = metric.ratio.score.unwrap().value;
        assert!(score > 0.0);
        assert!(score < 1.0);
    }

    #[test]
    fn test_unknown_type_strict_policy_should_return_error() {
        let mid = ModuleId(1);
        let oid_a = ObjectId(1);
        let mut obj_a = make_object(oid_a, mid, "A", ObjectKind::Type(TypeKind::Struct));
        obj_a.constructors = vec![Constructor {
            name: Some("new".to_string()),
            parameters: vec![Parameter {
                name: "x".to_string(),
                ty: TypeRef::Unknown {
                    raw: "UnknownType".to_string(),
                },
            }],
        }];
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid_a])],
            vec![obj_a],
            Vec::new(),
        );
        let result =
            CompositionalAbstractness::new().object_abstractness(&graph, oid_a, &strict_policy());
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_type_ignore_policy_should_exclude_from_score() {
        let mid = ModuleId(1);
        let oid_a = ObjectId(1);
        let mut obj_a = make_object(oid_a, mid, "A", ObjectKind::Type(TypeKind::Struct));
        obj_a.constructors = vec![Constructor {
            name: Some("new".to_string()),
            parameters: vec![Parameter {
                name: "x".to_string(),
                ty: TypeRef::Unknown {
                    raw: "UnknownType".to_string(),
                },
            }],
        }];
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid_a])],
            vec![obj_a],
            Vec::new(),
        );
        let result =
            CompositionalAbstractness::new().object_abstractness(&graph, oid_a, &ignore_policy());
        assert!(result.is_ok());
    }

    #[test]
    fn test_external_unknown_strict_policy_should_return_error() {
        let mid = ModuleId(1);
        let oid_a = ObjectId(1);
        let mut obj_a = make_object(oid_a, mid, "A", ObjectKind::Type(TypeKind::Struct));
        obj_a.constructors = vec![Constructor {
            name: Some("new".to_string()),
            parameters: vec![Parameter {
                name: "x".to_string(),
                ty: TypeRef::External {
                    name: QualifiedName("SomeExternal".to_string()),
                    abstraction_hint: ExternalAbstractionHint::Unknown,
                },
            }],
        }];
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid_a])],
            vec![obj_a],
            Vec::new(),
        );
        let result =
            CompositionalAbstractness::new().object_abstractness(&graph, oid_a, &strict_policy());
        assert!(result.is_err());
    }

    #[test]
    fn test_cycle_break_with_zero_should_use_zero() {
        let mid = ModuleId(1);
        let oid_a = ObjectId(1);
        let oid_b = ObjectId(2);
        let mut obj_a = make_object(oid_a, mid, "A", ObjectKind::Type(TypeKind::Struct));
        obj_a.constructors = vec![Constructor {
            name: None,
            parameters: vec![Parameter {
                name: "b".to_string(),
                ty: TypeRef::Internal(oid_b),
            }],
        }];
        let mut obj_b = make_object(oid_b, mid, "B", ObjectKind::Type(TypeKind::Struct));
        obj_b.constructors = vec![Constructor {
            name: None,
            parameters: vec![Parameter {
                name: "a".to_string(),
                ty: TypeRef::Internal(oid_a),
            }],
        }];
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid_a, oid_b])],
            vec![obj_a, obj_b],
            Vec::new(),
        );
        let policy = AnalysisPolicy {
            cycle_policy: CyclePolicy::BreakWithZero,
            ..Default::default()
        };
        let analyzer = CompositionalAbstractness::new();
        assert!(analyzer.object_abstractness(&graph, oid_a, &policy).is_ok());
        assert!(analyzer.object_abstractness(&graph, oid_b, &policy).is_ok());
    }

    #[test]
    fn test_cycle_error_policy_should_return_error() {
        let mid = ModuleId(1);
        let oid_a = ObjectId(1);
        let oid_b = ObjectId(2);
        let mut obj_a = make_object(oid_a, mid, "A", ObjectKind::Type(TypeKind::Struct));
        obj_a.constructors = vec![Constructor {
            name: None,
            parameters: vec![Parameter {
                name: "b".to_string(),
                ty: TypeRef::Internal(oid_b),
            }],
        }];
        let mut obj_b = make_object(oid_b, mid, "B", ObjectKind::Type(TypeKind::Struct));
        obj_b.constructors = vec![Constructor {
            name: None,
            parameters: vec![Parameter {
                name: "a".to_string(),
                ty: TypeRef::Internal(oid_a),
            }],
        }];
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid_a, oid_b])],
            vec![obj_a, obj_b],
            Vec::new(),
        );
        let policy = AnalysisPolicy {
            cycle_policy: CyclePolicy::Error,
            ..Default::default()
        };
        assert!(
            CompositionalAbstractness::new()
                .object_abstractness(&graph, oid_a, &policy)
                .is_err()
        );
    }

    #[test]
    fn test_validate_graph_unknown_module_reference_should_fail() {
        let mid1 = ModuleId(1);
        let mid2 = ModuleId(2);
        let oid = ObjectId(1);
        let graph = make_graph(
            vec![make_source_module(mid1, "m1", vec![])],
            vec![make_object(oid, mid2, "A", ObjectKind::Constant)],
            Vec::new(),
        );
        assert!(validate_graph(&graph).is_err());
    }

    #[test]
    fn test_validate_graph_unknown_object_in_module_should_fail() {
        let mid = ModuleId(1);
        let oid = ObjectId(1);
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid])],
            vec![],
            Vec::new(),
        );
        assert!(validate_graph(&graph).is_err());
    }

    #[test]
    fn test_validate_graph_unknown_dependency_source_should_fail() {
        let mid = ModuleId(1);
        let oid = ObjectId(1);
        let oid_unknown = ObjectId(999);
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid])],
            vec![make_object(oid, mid, "A", ObjectKind::Constant)],
            vec![DependencyEdge {
                source: oid_unknown,
                target: TypeRef::Primitive {
                    name: "int".to_string(),
                },
                kind: crate::model::DependencyKind::Calls,
                occurrence_count: 1,
            }],
        );
        assert!(validate_graph(&graph).is_err());
    }

    #[test]
    fn test_validate_graph_unknown_internal_dependency_target_should_fail() {
        let mid = ModuleId(1);
        let oid = ObjectId(1);
        let oid_unknown = ObjectId(999);
        let graph = make_graph(
            vec![make_source_module(mid, "m", vec![oid])],
            vec![make_object(oid, mid, "A", ObjectKind::Constant)],
            vec![DependencyEdge {
                source: oid,
                target: TypeRef::Internal(oid_unknown),
                kind: crate::model::DependencyKind::Calls,
                occurrence_count: 1,
            }],
        );
        assert!(validate_graph(&graph).is_err());
    }

    #[test]
    fn test_module_is_test_source_variant_should_return_false() {
        let m = make_source_module(ModuleId(1), "src", vec![]);
        assert!(!m.is_test());
    }

    #[test]
    fn test_module_is_test_test_variant_should_return_true() {
        let m = make_test_module(ModuleId(1), "tests", vec![]);
        assert!(m.is_test());
    }
}
