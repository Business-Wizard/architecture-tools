use std::collections::{BTreeMap, BTreeSet, HashMap};

use architecture_core::model::{
    ArchitectureGraph, CodeObject, DependencyEdge, DependencyKind, ExternalAbstractionHint, Module,
    ModuleEdge, ModuleId, ObjectId, ObjectKind, Operation, OperationAbstraction, OperationKind,
    QualifiedName, TraitLikeKind, TypeKind, TypeRef,
};
use camino::Utf8PathBuf;
use lang_core::{ClassDef, ModuleDep};

type ObjectIndex = (
    BTreeMap<ObjectId, CodeObject>,
    HashMap<String, ObjectId>,
    HashMap<ModuleId, BTreeSet<ObjectId>>,
);

#[allow(dead_code)]
pub struct ArchitectureGraphBuilder;

#[allow(dead_code)]
impl ArchitectureGraphBuilder {
    pub fn build(
        deps: &[ModuleDep],
        class_defs: &[ClassDef],
        source_files: &[Utf8PathBuf],
        namer: &dyn lang_core::ModuleNamer,
    ) -> ArchitectureGraph {
        let module_map = build_module_map(source_files, namer);
        let (file_to_module_id, module_name_to_id) = collect_modules(deps, class_defs, &module_map);
        let (objects, qname_to_object_id, mut module_object_ids) =
            build_objects(class_defs, &module_name_to_id);
        let dependencies = build_dependencies(class_defs, &qname_to_object_id);
        let module_edges = build_module_edges(deps, &module_name_to_id);
        let modules = assemble_modules(&file_to_module_id, &mut module_object_ids);

        ArchitectureGraph {
            modules,
            objects,
            dependencies,
            module_edges,
        }
    }
}

fn collect_modules(
    deps: &[ModuleDep],
    class_defs: &[ClassDef],
    module_map: &HashMap<String, Utf8PathBuf>,
) -> (HashMap<Utf8PathBuf, ModuleId>, HashMap<String, ModuleId>) {
    let mut file_to_module_id: HashMap<Utf8PathBuf, ModuleId> = HashMap::new();
    let mut next_id: u32 = 0;

    let mut register = |file: &Utf8PathBuf| {
        if !file_to_module_id.contains_key(file) {
            file_to_module_id.insert(file.clone(), ModuleId(next_id));
            next_id += 1;
        }
    };

    for dep in deps {
        if let Some(f) = resolve_module(module_map, dep.from.as_str()) {
            register(f);
        }
        if let Some(f) = resolve_module(module_map, dep.to.as_str()) {
            register(f);
        }
    }
    for def in class_defs {
        if let Some(f) = resolve_module(module_map, &def.module) {
            register(f);
        }
    }

    let mut module_name_to_id: HashMap<String, ModuleId> = HashMap::new();
    let mut register_name = |name: &str| {
        if let Some(f) = resolve_module(module_map, name)
            && let Some(&mid) = file_to_module_id.get(f)
        {
            module_name_to_id.entry(name.to_owned()).or_insert(mid);
        }
    };
    for dep in deps {
        register_name(dep.from.as_str());
        register_name(dep.to.as_str());
    }
    for def in class_defs {
        register_name(&def.module);
    }

    (file_to_module_id, module_name_to_id)
}

fn build_objects(
    class_defs: &[ClassDef],
    module_name_to_id: &HashMap<String, ModuleId>,
) -> ObjectIndex {
    let mut objects: BTreeMap<ObjectId, CodeObject> = BTreeMap::new();
    let mut qname_to_object_id: HashMap<String, ObjectId> = HashMap::new();
    let mut module_object_ids: HashMap<ModuleId, BTreeSet<ObjectId>> = HashMap::new();

    for (i, def) in class_defs.iter().enumerate() {
        let oid = ObjectId(u32::try_from(i).expect("class_defs length fits u32"));
        let qname = format!("{}.{}", def.module, def.name);
        let module_id = module_name_to_id
            .get(&def.module)
            .copied()
            .unwrap_or(ModuleId(u32::MAX));

        let operations = def
            .methods
            .iter()
            .map(|m| Operation {
                name: m.clone(),
                kind: OperationKind::Unknown,
                abstraction: OperationAbstraction::Unknown,
                parameters: vec![],
                return_type: None,
            })
            .collect();

        objects.insert(
            oid,
            CodeObject {
                id: oid,
                module_id,
                name: QualifiedName(qname.clone()),
                kind: object_kind_from_bases(&def.bases),
                constructors: vec![],
                operations,
            },
        );
        qname_to_object_id.insert(qname, oid);
        module_object_ids.entry(module_id).or_default().insert(oid);
    }

    (objects, qname_to_object_id, module_object_ids)
}

fn build_dependencies(
    class_defs: &[ClassDef],
    qname_to_object_id: &HashMap<String, ObjectId>,
) -> Vec<DependencyEdge> {
    let mut dependencies: Vec<DependencyEdge> = Vec::new();

    for def in class_defs {
        let src_qname = format!("{}.{}", def.module, def.name);
        let Some(&src_oid) = qname_to_object_id.get(&src_qname) else {
            continue;
        };

        for base in &def.bases {
            let target = resolve_type_ref(base, qname_to_object_id);
            dependencies.push(DependencyEdge {
                source: src_oid,
                target,
                kind: DependencyKind::Inherits,
                occurrence_count: 1,
            });
        }

        for dep_name in &def.class_deps {
            let target = resolve_type_ref(dep_name, qname_to_object_id);
            dependencies.push(DependencyEdge {
                source: src_oid,
                target,
                kind: DependencyKind::Calls,
                occurrence_count: 1,
            });
        }
    }

    dependencies
}

fn build_module_edges(
    deps: &[ModuleDep],
    module_name_to_id: &HashMap<String, ModuleId>,
) -> Vec<ModuleEdge> {
    deps.iter()
        .filter_map(|dep| {
            let from = *module_name_to_id.get(dep.from.as_str())?;
            let to = *module_name_to_id.get(dep.to.as_str())?;
            Some(ModuleEdge { from, to })
        })
        .collect()
}

fn assemble_modules(
    file_to_module_id: &HashMap<Utf8PathBuf, ModuleId>,
    module_object_ids: &mut HashMap<ModuleId, BTreeSet<ObjectId>>,
) -> BTreeMap<ModuleId, Module> {
    let mut modules: BTreeMap<ModuleId, Module> = BTreeMap::new();
    for (file, &mid) in file_to_module_id {
        let name = path_to_module_name(file);
        let object_ids = module_object_ids.remove(&mid).unwrap_or_default();
        let module = if is_test_path(file) {
            Module::Test {
                id: mid,
                name: QualifiedName(name),
                file_path: file.clone(),
                object_ids,
            }
        } else {
            Module::Source {
                id: mid,
                name: QualifiedName(name),
                file_path: file.clone(),
                object_ids,
            }
        };
        modules.insert(mid, module);
    }
    modules
}

fn resolve_type_ref(name: &str, qname_to_object_id: &HashMap<String, ObjectId>) -> TypeRef {
    if let Some(&dst_oid) = qname_to_object_id.get(name) {
        TypeRef::Internal(dst_oid)
    } else {
        TypeRef::External {
            name: QualifiedName(name.to_owned()),
            abstraction_hint: ExternalAbstractionHint::Unknown,
        }
    }
}

fn object_kind_from_bases(bases: &[String]) -> ObjectKind {
    for base in bases {
        let short = base.split('.').next_back().unwrap_or(base.as_str());
        match short {
            "Protocol" => return ObjectKind::TraitLike(TraitLikeKind::Protocol),
            "ABC" | "ABCMeta" => return ObjectKind::TraitLike(TraitLikeKind::AbstractClass),
            "Trait" => return ObjectKind::TraitLike(TraitLikeKind::RustTrait),
            _ => {}
        }
    }
    ObjectKind::Type(TypeKind::Class)
}

fn is_test_path(path: &Utf8PathBuf) -> bool {
    let s = path.as_str();
    s.contains("/tests/")
        || s.contains("/test_")
        || s.ends_with("_test.py")
        || s.ends_with("_test.rs")
        || s.starts_with("tests/")
        || s.starts_with("test_")
}

fn path_to_module_name(file: &Utf8PathBuf) -> String {
    file.as_str()
        .trim_end_matches(".py")
        .trim_end_matches(".rs")
        .replace('/', ".")
}

fn build_module_map(
    source_files: &[Utf8PathBuf],
    namer: &dyn lang_core::ModuleNamer,
) -> HashMap<String, Utf8PathBuf> {
    let mut map = HashMap::new();
    for file in source_files {
        let dotted = namer.path_to_module_name(std::path::Path::new(file.as_str()));
        let parts: Vec<&str> = dotted.split('.').collect();
        for start in 0..parts.len() {
            map.entry(parts[start..].join("."))
                .or_insert_with(|| file.clone());
        }
    }
    map
}

fn resolve_module<'a>(
    module_map: &'a HashMap<String, Utf8PathBuf>,
    name: &str,
) -> Option<&'a Utf8PathBuf> {
    let mut s = name;
    loop {
        if let Some(v) = module_map.get(s) {
            return Some(v);
        }
        match s.find('.') {
            Some(pos) => s = &s[pos + 1..],
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python_namer() -> py_analyzer::PythonAnalyzer {
        py_analyzer::PythonAnalyzer
    }

    fn make_def(module: &str, name: &str, bases: Vec<&str>, deps: Vec<&str>) -> ClassDef {
        ClassDef {
            module: module.to_string(),
            name: name.to_string(),
            bases: bases.into_iter().map(str::to_string).collect(),
            attributes: vec![],
            methods: vec![],
            class_deps: deps.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn test_empty_inputs_should_produce_empty_graph() {
        let actual = ArchitectureGraphBuilder::build(&[], &[], &[], &python_namer());
        let expected = ArchitectureGraph {
            modules: BTreeMap::new(),
            objects: BTreeMap::new(),
            dependencies: vec![],
            module_edges: vec![],
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_module_dep_with_known_files_should_create_two_modules() {
        let files = vec![
            Utf8PathBuf::from("order.py"),
            Utf8PathBuf::from("billing.py"),
        ];
        let deps = vec![ModuleDep {
            from: "order".into(),
            to: "billing".into(),
        }];
        let actual = ArchitectureGraphBuilder::build(&deps, &[], &files, &python_namer());
        assert_eq!(actual.modules.len(), 2);
    }

    #[test]
    fn test_test_file_path_should_produce_test_module_variant() {
        let files = vec![Utf8PathBuf::from("tests/test_order.py")];
        let deps = vec![ModuleDep {
            from: "test_order".into(),
            to: "test_order".into(),
        }];
        let actual = ArchitectureGraphBuilder::build(&deps, &[], &files, &python_namer());
        let module = actual.modules.values().next().unwrap();
        assert!(module.is_test());
    }

    #[test]
    fn test_class_def_should_create_code_object_in_module() {
        let files = vec![Utf8PathBuf::from("domain.py")];
        let defs = vec![make_def("domain", "Order", vec![], vec![])];
        let actual = ArchitectureGraphBuilder::build(&[], &defs, &files, &python_namer());
        assert_eq!(actual.objects.len(), 1);
        let obj = actual.objects.values().next().unwrap();
        assert_eq!(obj.name, QualifiedName("domain.Order".to_owned()));
    }

    #[test]
    fn test_protocol_base_should_produce_traitlike_kind() {
        let files = vec![Utf8PathBuf::from("domain.py")];
        let defs = vec![make_def("domain", "Repo", vec!["Protocol"], vec![])];
        let actual = ArchitectureGraphBuilder::build(&[], &defs, &files, &python_namer());
        let obj = actual.objects.values().next().unwrap();
        assert_eq!(obj.kind, ObjectKind::TraitLike(TraitLikeKind::Protocol));
    }

    #[test]
    fn test_internal_base_should_produce_internal_dependency_edge() {
        let files = vec![Utf8PathBuf::from("domain.py")];
        let defs = vec![
            make_def("domain", "Base", vec![], vec![]),
            make_def("domain", "Service", vec!["domain.Base"], vec![]),
        ];
        let actual = ArchitectureGraphBuilder::build(&[], &defs, &files, &python_namer());
        let edge = actual
            .dependencies
            .iter()
            .find(|e| matches!(e.target, TypeRef::Internal(_)));
        assert!(edge.is_some());
        assert_eq!(edge.unwrap().kind, DependencyKind::Inherits);
    }

    #[test]
    fn test_external_base_should_produce_external_dependency_edge() {
        let files = vec![Utf8PathBuf::from("domain.py")];
        let defs = vec![make_def("domain", "Repo", vec!["Protocol"], vec![])];
        let actual = ArchitectureGraphBuilder::build(&[], &defs, &files, &python_namer());
        let edge = actual
            .dependencies
            .iter()
            .find(|e| matches!(e.target, TypeRef::External { .. }));
        assert!(edge.is_some());
    }
}
