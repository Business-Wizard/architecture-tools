use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use architecture_core::model::{
    ArchitectureGraph, CodeObject, DependencyEdge, DependencyKind, ExternalAbstractionHint,
    FunctionKind, Module, ModuleId, ObjectId, ObjectKind, QualifiedName, TraitLikeKind, TypeKind,
    TypeRef,
};
use camino::Utf8PathBuf;

use crate::python_ast::{
    ClassKind, ParsedFile, extract_module_names, find_functions, find_imports,
};

struct Builder {
    next_module_id: u32,
    next_object_id: u32,
    modules: BTreeMap<ModuleId, Module>,
    objects: BTreeMap<ObjectId, CodeObject>,
    dependencies: Vec<DependencyEdge>,
    name_to_module: HashMap<String, ModuleId>,
    module_sentinel: HashMap<ModuleId, ObjectId>,
}

impl Builder {
    fn new() -> Self {
        Self {
            next_module_id: 0,
            next_object_id: 0,
            modules: BTreeMap::new(),
            objects: BTreeMap::new(),
            dependencies: Vec::new(),
            name_to_module: HashMap::new(),
            module_sentinel: HashMap::new(),
        }
    }

    fn alloc_module(&mut self) -> ModuleId {
        let id = ModuleId(self.next_module_id);
        self.next_module_id += 1;
        id
    }

    fn alloc_object(&mut self) -> ObjectId {
        let id = ObjectId(self.next_object_id);
        self.next_object_id += 1;
        id
    }

    fn path_to_qualified_name(file: &Utf8PathBuf, repo_root: &Path) -> String {
        let rel = file
            .as_std_path()
            .strip_prefix(repo_root)
            .unwrap_or(file.as_std_path());
        rel.to_string_lossy()
            .trim_end_matches(".py")
            .replace(['/', '\\'], ".")
    }

    fn register_modules(&mut self, source_files: &[Utf8PathBuf], repo_root: &Path) {
        for file in source_files {
            let module_name = Self::path_to_qualified_name(file, repo_root);
            let mid = self.alloc_module();
            let sentinel_id = self.alloc_object();

            let sentinel = CodeObject {
                id: sentinel_id,
                module_id: mid,
                name: QualifiedName(format!("{module_name}.__module__")),
                kind: ObjectKind::Unknown,
                constructors: vec![],
                operations: vec![],
            };
            self.objects.insert(sentinel_id, sentinel);

            let module = Module {
                id: mid,
                name: QualifiedName(module_name.clone()),
                object_ids: BTreeSet::from([sentinel_id]),
            };
            self.modules.insert(mid, module);
            self.name_to_module.insert(module_name, mid);
            self.module_sentinel.insert(mid, sentinel_id);
        }
    }

    fn add_objects_from_file(&mut self, file: &Utf8PathBuf, repo_root: &Path, source: &[u8]) {
        let module_name = Self::path_to_qualified_name(file, repo_root);
        let Some(&mid) = self.name_to_module.get(&module_name) else {
            return;
        };

        let Some(parsed) = ParsedFile::parse(source) else {
            return;
        };

        for class in find_classes_with_names(&parsed) {
            let oid = self.alloc_object();
            let kind = match class.kind {
                ClassKind::Protocol => ObjectKind::TraitLike(TraitLikeKind::Protocol),
                ClassKind::Abstract => ObjectKind::TraitLike(TraitLikeKind::AbstractClass),
                ClassKind::Concrete => ObjectKind::Type(TypeKind::Class),
            };
            let obj = CodeObject {
                id: oid,
                module_id: mid,
                name: QualifiedName(format!("{}.{}", module_name, class.name)),
                kind,
                constructors: vec![],
                operations: vec![],
            };
            self.objects.insert(oid, obj);
            self.modules.get_mut(&mid).unwrap().object_ids.insert(oid);
        }

        for func in find_functions(&parsed) {
            if func.is_method || func.is_constructor {
                continue;
            }
            let oid = self.alloc_object();
            let obj = CodeObject {
                id: oid,
                module_id: mid,
                name: QualifiedName(format!("{}.{}", module_name, func.name)),
                kind: ObjectKind::Function(FunctionKind::FreeFunction),
                constructors: vec![],
                operations: vec![],
            };
            self.objects.insert(oid, obj);
            self.modules.get_mut(&mid).unwrap().object_ids.insert(oid);
        }

        for import in find_imports(&parsed) {
            for target_module_name in extract_module_names(&import.module_path) {
                let source_sentinel = self.module_sentinel[&mid];
                let target = match self.name_to_module.get(&target_module_name).copied() {
                    Some(target_mid) => TypeRef::Internal(self.module_sentinel[&target_mid]),
                    None => TypeRef::External {
                        name: QualifiedName(target_module_name),
                        abstraction_hint: ExternalAbstractionHint::Unknown,
                    },
                };
                self.dependencies.push(DependencyEdge {
                    source: source_sentinel,
                    target,
                    kind: DependencyKind::Unknown,
                    occurrence_count: 1,
                });
            }
        }
    }

    fn build(self) -> ArchitectureGraph {
        ArchitectureGraph {
            modules: self.modules,
            objects: self.objects,
            dependencies: self.dependencies,
        }
    }
}

struct ClassWithName {
    name: String,
    kind: ClassKind,
}

fn find_classes_with_names(parsed: &ParsedFile) -> Vec<ClassWithName> {
    let mut results = Vec::new();
    collect_named_classes(parsed.root(), &parsed.source, &mut results);
    results
}

fn collect_named_classes(node: tree_sitter::Node<'_>, source: &[u8], out: &mut Vec<ClassWithName>) {
    if node.kind() == "class_definition" {
        if let Some(name_node) = node.child_by_field_name("name") {
            if let Ok(name) = name_node.utf8_text(source) {
                let kind = classify_class_kind(node, source);
                out.push(ClassWithName {
                    name: name.to_string(),
                    kind,
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_named_classes(child, source, out);
    }
}

fn classify_class_kind(node: tree_sitter::Node<'_>, source: &[u8]) -> ClassKind {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "argument_list" {
            let mut c2 = child.walk();
            for base in child.children(&mut c2) {
                let text = base.utf8_text(source).unwrap_or("");
                if text == "Protocol" || text.ends_with(".Protocol") {
                    return ClassKind::Protocol;
                }
                if text == "ABC" || text.ends_with(".ABC") {
                    return ClassKind::Abstract;
                }
            }
        }
    }
    ClassKind::Concrete
}

/// Build an `ArchitectureGraph` from Python source files.
///
/// Relative imports produce no dependency edges — only absolute imports are resolved.
pub fn build_architecture_graph(
    source_files: &[Utf8PathBuf],
    repo_root: &Path,
) -> ArchitectureGraph {
    let mut builder = Builder::new();
    builder.register_modules(source_files, repo_root);

    for file in source_files {
        if let Ok(source) = std::fs::read(file.as_std_path()) {
            builder.add_objects_from_file(file, repo_root, &source);
        }
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use architecture_core::analyzer::validate_graph;
    use std::io::Write as _;

    fn write_file(dir: &std::path::Path, name: &str, content: &str) -> Utf8PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        Utf8PathBuf::try_from(path).unwrap()
    }

    #[test]
    fn test_build_two_file_project_should_produce_valid_graph() {
        let dir = tempfile::tempdir().unwrap();
        let order = write_file(dir.path(), "order.py", "class Order:\n    pass\n");
        let billing = write_file(
            dir.path(),
            "billing.py",
            "from order import Order\nclass BillingService:\n    pass\n",
        );
        let graph = build_architecture_graph(&[order, billing], dir.path());
        assert!(validate_graph(&graph).is_ok());
        assert_eq!(graph.modules.len(), 2);
    }

    #[test]
    fn test_build_with_relative_import_should_produce_no_internal_edge() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "a.py", "class A:\n    pass\n");
        let b = write_file(dir.path(), "b.py", "from .a import A\nclass B:\n    pass\n");
        let graph = build_architecture_graph(&[a, b], dir.path());
        assert!(validate_graph(&graph).is_ok());
        let internal_edges = graph
            .dependencies
            .iter()
            .filter(|e| matches!(e.target, TypeRef::Internal(_)))
            .count();
        assert_eq!(internal_edges, 0);
    }

    #[test]
    fn test_module_qualified_names_derived_from_paths() {
        let dir = tempfile::tempdir().unwrap();
        let f = write_file(dir.path(), "my_module.py", "");
        let graph = build_architecture_graph(&[f], dir.path());
        let names: Vec<_> = graph.modules.values().map(|m| m.name.0.as_str()).collect();
        assert!(names.iter().any(|n| *n == "my_module"));
    }
}
