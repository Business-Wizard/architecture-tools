use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

/// Unique identifier for a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId(pub u32);

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0) // delegating to u32
    }
}

/// Unique identifier for a code object (type, function, etc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u32);

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A fully qualified name (e.g., `mymodule.MyClass.method`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedName(pub String);

impl fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The kind of abstraction a type represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Struct,
    Enum(EnumKind),
    Class,
    TypeAlias,
}

/// The kind of enum definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumKind {
    AlgebraicDataType,
    DataOnly,
    Flags,
    Unknown,
}

/// The kind of function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    FreeFunction,
    StaticFunction,
    AssociatedFunction,
    Closure,
    Unknown,
}

/// The kind of trait-like construct (trait, interface, protocol, etc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraitLikeKind {
    RustTrait,
    Interface,
    Protocol,
    AbstractClass,
}

/// The overall kind of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Constant,
    Type(TypeKind),
    Function(FunctionKind),
    TraitLike(TraitLikeKind),
    Unknown,
}

/// A reference to a type (could be internal or external).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Internal(ObjectId),
    External {
        name: QualifiedName,
        abstraction_hint: ExternalAbstractionHint,
    },
    Primitive {
        name: String,
    },
    Unknown {
        raw: String,
    },
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeRef::Internal(id) => write!(f, "internal#{id}"),
            TypeRef::External { name, .. } => write!(f, "{name}"),
            TypeRef::Primitive { name } => write!(f, "{name}"),
            TypeRef::Unknown { raw } => write!(f, "unknown({raw})"),
        }
    }
}

/// A hint about whether an external type is abstract or concrete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalAbstractionHint {
    Abstract,
    Concrete,
    Unknown,
}

/// The kind of dependency relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    ConstructorParameter,
    Field,
    OperationParameter,
    ReturnType,
    Calls,
    Instantiates,
    Inherits,
    Implements,
    UsesTrait,
    GenericBound,
    AssociatedTypeBound,
    Unknown,
}

/// The level of abstraction in an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationAbstraction {
    Concrete,
    Abstract,
    Unknown,
}

/// The kind of operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    InstanceMethod,
    StaticMethod,
    AssociatedFunction,
    FreeFunction,
    TraitRequirement,
    InterfaceMember,
    ProtocolRequirement,
    AbstractMethod,
    Unknown,
}

/// A function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub ty: TypeRef,
}

/// A constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constructor {
    pub name: Option<String>,
    pub parameters: Vec<Parameter>,
}

/// An operation (method, function, etc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub name: String,
    pub kind: OperationKind,
    pub abstraction: OperationAbstraction,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeRef>,
}

/// A module in the architecture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Module {
    Source {
        id: ModuleId,
        name: QualifiedName,
        object_ids: BTreeSet<ObjectId>,
    },
    Test {
        id: ModuleId,
        name: QualifiedName,
        object_ids: BTreeSet<ObjectId>,
    },
}

impl Module {
    #[must_use]
    pub fn id(&self) -> ModuleId {
        match self {
            Module::Source { id, .. } | Module::Test { id, .. } => *id,
        }
    }

    #[must_use]
    pub fn name(&self) -> &QualifiedName {
        match self {
            Module::Source { name, .. } | Module::Test { name, .. } => name,
        }
    }

    #[must_use]
    pub fn object_ids(&self) -> &BTreeSet<ObjectId> {
        match self {
            Module::Source { object_ids, .. } | Module::Test { object_ids, .. } => object_ids,
        }
    }

    #[must_use]
    pub fn is_test(&self) -> bool {
        matches!(self, Module::Test { .. })
    }
}

/// A code object (type, function, constant, etc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeObject {
    pub id: ObjectId,
    pub module_id: ModuleId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub constructors: Vec<Constructor>,
    pub operations: Vec<Operation>,
}

/// An edge in the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyEdge {
    pub source: ObjectId,
    pub target: TypeRef,
    pub kind: DependencyKind,
    pub occurrence_count: usize,
}

/// The complete architecture graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureGraph {
    pub modules: BTreeMap<ModuleId, Module>,
    pub objects: BTreeMap<ObjectId, CodeObject>,
    pub dependencies: Vec<DependencyEdge>,
}

impl Error for ObjectId {}

impl Error for TypeRef {}
