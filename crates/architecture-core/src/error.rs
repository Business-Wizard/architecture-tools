use crate::model::{ModuleId, ObjectId, TypeRef};
use thiserror::Error;

/// Errors that can occur during architecture analysis.
#[derive(Debug, Error)]
pub enum ArchitectureError {
    #[error("unknown module: {0}")]
    UnknownModule(ModuleId),

    #[error("unknown object: {0}")]
    UnknownObject(ObjectId),

    #[error("object {object_id} references unknown module {module_id}")]
    ObjectReferencesUnknownModule {
        object_id: ObjectId,
        module_id: ModuleId,
    },

    #[error("module {module_id} references unknown object {object_id}")]
    ModuleReferencesUnknownObject {
        module_id: ModuleId,
        object_id: ObjectId,
    },

    #[error("dependency references unknown source object {0}")]
    DependencyReferencesUnknownSource(ObjectId),

    #[error("dependency from {source} references unknown internal target {target}")]
    DependencyReferencesUnknownTarget { source: ObjectId, target: TypeRef },

    #[error("unknown type: {raw}")]
    UnknownType { raw: String },

    #[error("unknown external abstraction for type {ty}")]
    UnknownExternalAbstraction { ty: TypeRef },

    #[error("unknown operation abstraction for operation {operation_name} on object {object_id}")]
    UnknownOperationAbstraction {
        object_id: ObjectId,
        operation_name: String,
    },

    #[error("abstractness cycle detected at object {object_id}")]
    AbstractnessCycle { object_id: ObjectId },

    #[error("invalid score value {value}: must be in [0.0, 1.0]")]
    InvalidScore { value: f64 },

    #[error("invalid model invariant: {message}")]
    InvalidModelInvariant { message: String },
}

/// Result type for architecture operations.
pub type Result<T> = std::result::Result<T, ArchitectureError>;
