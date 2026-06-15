use std::collections::{HashMap, HashSet};
use std::path::Path;

use tree_sitter::{Node, Parser, Tree};

use crate::error::InspectorError;
use lang_core::{ClassDef, ModuleDep};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn extract(package_path: &Path) -> Result<(Vec<ModuleDep>, Vec<ClassDef>), InspectorError> {
    let py_files = collect_python_files(package_path)?;

    let mut raw_classes: Vec<RawClass> = Vec::new();
    let mut module_deps: Vec<ModuleDep> = Vec::new();

    for file in &py_files {
        let source = std::fs::read(file).map_err(InspectorError::Io)?;
        let rel = file.strip_prefix(package_path).unwrap_or(file);
        let rel_str = rel.to_string_lossy();
        let without_ext = rel_str.trim_end_matches(".py");
        let dotted = without_ext.replace(['/', '\\'], ".");
        let module_name = dotted
            .strip_suffix(".__init__")
            .unwrap_or(&dotted)
            .to_string();

        let Some(parsed) = ParsedFile::parse(&source) else {
            continue;
        };

        collect_module_deps(&parsed, &module_name, &mut module_deps);
        collect_raw_classes(&parsed, &module_name, &mut raw_classes);
    }

    // Build per-class qualified-name lookup: "module.ClassName" → qualified node ID.
    // Used by resolve_class_deps so deps are emitted as qualified strings.
    let qualified_class_set: HashSet<String> = raw_classes
        .iter()
        .map(|c| format!("{}.{}", c.module, c.name))
        .collect();
    let all_class_names: HashSet<String> = raw_classes.iter().map(|c| c.name.clone()).collect();

    let classes = raw_classes
        .into_iter()
        .map(|rc| resolve_class_deps(rc, &all_class_names, &qualified_class_set))
        .collect();

    Ok((module_deps, classes))
}

// ---------------------------------------------------------------------------
// File walking
// ---------------------------------------------------------------------------

fn collect_python_files(root: &Path) -> Result<Vec<std::path::PathBuf>, InspectorError> {
    let mut files = Vec::new();
    for result in ignore::Walk::new(root) {
        let entry = result.map_err(|e| InspectorError::Io(std::io::Error::other(e.to_string())))?;
        let path = entry.path().to_path_buf();
        if path.extension().and_then(|e| e.to_str()) == Some("py") {
            files.push(path);
        }
    }
    Ok(files)
}

// ---------------------------------------------------------------------------
// Parsed file wrapper
// ---------------------------------------------------------------------------

struct ParsedFile {
    source: Vec<u8>,
    tree: Tree,
}

impl ParsedFile {
    fn parse(source: &[u8]) -> Option<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .ok()?;
        let tree = parser.parse(source, None)?;
        Some(Self {
            source: source.to_vec(),
            tree,
        })
    }

    fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }
}

// ---------------------------------------------------------------------------
// Module-level import extraction
// Ported from awt/src/python_ast.rs: find_imports + extract_module_names.
// ---------------------------------------------------------------------------

fn collect_module_deps(parsed: &ParsedFile, module_name: &str, out: &mut Vec<ModuleDep>) {
    let imports = find_import_statements(parsed.root(), &parsed.source);
    for stmt in imports {
        for target in extract_module_names(&stmt, module_name) {
            out.push(ModuleDep {
                from: module_name.into(),
                to: target.into(),
            });
        }
    }
}

fn find_import_statements(node: Node<'_>, source: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    collect_import_statements(node, source, &mut out);
    out
}

fn collect_import_statements(node: Node<'_>, source: &[u8], out: &mut Vec<String>) {
    if matches!(node.kind(), "import_statement" | "import_from_statement") {
        out.push(node.utf8_text(source).unwrap_or("").to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_import_statements(child, source, out);
    }
}

fn extract_module_names(statement: &str, current_module: &str) -> Vec<String> {
    let s = statement.trim();
    if let Some(rest) = s.strip_prefix("from ") {
        let module_part = rest.split_whitespace().next().unwrap_or("");
        let dot_count = module_part
            .find(|c: char| c != '.')
            .unwrap_or(module_part.len());
        if dot_count > 0 {
            // Relative import: resolve against current_module's ancestor package.
            // When the module is flat (no dots), a single-dot import refers to a sibling
            // module in the same scan root, so the resolved name is just the suffix.
            let suffix = &module_part[dot_count..];
            if let Some(anchor) = package_anchor(current_module, dot_count) {
                let resolved = if suffix.is_empty() {
                    anchor
                } else {
                    format!("{anchor}.{suffix}")
                };
                return vec![resolved];
            }
            if dot_count == 1 && !suffix.is_empty() {
                return vec![suffix.to_string()];
            }
            return vec![];
        }
        let module =
            module_part.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_');
        if module.is_empty() {
            return vec![];
        }
        vec![module.to_string()]
    } else if let Some(rest) = s.strip_prefix("import ") {
        rest.split(',')
            .filter_map(|seg| {
                let name = seg.split_whitespace().next().unwrap_or("");
                if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                }
            })
            .collect()
    } else {
        vec![]
    }
}

/// Return the dotted package prefix after going up `levels` from `module`.
/// One dot  → strip the last component (the file itself) → the containing package.
/// Two dots → strip two components, etc.
fn package_anchor(module: &str, levels: usize) -> Option<String> {
    let parts: Vec<&str> = module.split('.').collect();
    // `levels` dots means go up `levels` components (1 dot = own package = drop 1).
    if parts.len() < levels {
        return None;
    }
    let anchor_parts = &parts[..parts.len() - levels];
    if anchor_parts.is_empty() {
        None
    } else {
        Some(anchor_parts.join("."))
    }
}

// ---------------------------------------------------------------------------
// Class analysis
// ---------------------------------------------------------------------------

struct RawClass {
    module: String,
    name: String,
    bases: Vec<String>,
    attributes: Vec<String>,
    methods: Vec<String>,
    /// Identifiers referenced anywhere in the class body (for `class_deps` resolution).
    referenced_names: Vec<String>,
    /// Maps imported symbol name → the qualified module it came from.
    /// Built from file-level `from X import Y` statements so deps can be qualified.
    imported_names: HashMap<String, String>,
}

fn collect_raw_classes(parsed: &ParsedFile, module_name: &str, out: &mut Vec<RawClass>) {
    let imports = find_import_statements(parsed.root(), &parsed.source);
    let imported_names = build_name_to_module_map(&imports, module_name);

    let root = parsed.root();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "class_definition"
            && let Some(rc) =
                extract_raw_class(child, &parsed.source, module_name, imported_names.clone())
        {
            out.push(rc);
        }
    }
}

fn extract_raw_class(
    node: Node<'_>,
    source: &[u8],
    module_name: &str,
    imported_names: HashMap<String, String>,
) -> Option<RawClass> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?
        .to_string();

    let bases = extract_bases(node, source);
    let body = node.child_by_field_name("body");
    let attributes = body.map_or_else(Vec::new, |b| extract_self_attributes(b, source));
    let methods = body.map_or_else(Vec::new, |b| extract_method_names(b, source));
    let referenced_names = body.map_or_else(Vec::new, |b| collect_identifiers(b, source));

    Some(RawClass {
        module: module_name.to_string(),
        name,
        bases,
        attributes,
        methods,
        referenced_names,
        imported_names,
    })
}

/// Builds a map of `symbol_name → qualified_module` from file-level import statements.
/// e.g. `from .order import Order` in module `billing` → `{"Order": "order"}`.
fn build_name_to_module_map(
    statements: &[String],
    current_module: &str,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for stmt in statements {
        let s = stmt.trim();
        if let Some(rest) = s.strip_prefix("from ") {
            let mut parts = rest.splitn(2, " import ");
            let Some(module_part) = parts.next() else {
                continue;
            };
            let Some(names_part) = parts.next() else {
                continue;
            };
            // Resolve the module the same way extract_module_names does.
            let resolved_modules = extract_module_names(stmt, current_module);
            let Some(resolved_module) = resolved_modules.into_iter().next() else {
                continue;
            };
            let _ = module_part; // used indirectly via extract_module_names
            for name in names_part.split(',') {
                let symbol = name
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c: char| c == '(' || c == ')');
                if !symbol.is_empty() {
                    map.insert(symbol.to_string(), resolved_module.clone());
                }
            }
        }
    }
    map
}

fn extract_bases(class_node: Node<'_>, source: &[u8]) -> Vec<String> {
    let Some(superclasses) = class_node.child_by_field_name("superclasses") else {
        return vec![];
    };
    let mut bases = Vec::new();
    let mut cursor = superclasses.walk();
    for child in superclasses.children(&mut cursor) {
        if matches!(child.kind(), "identifier" | "attribute")
            && let Ok(text) = child.utf8_text(source)
        {
            bases.push(text.to_string());
        }
    }
    bases
}

fn extract_self_attributes(body: Node<'_>, source: &[u8]) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "function_definition" {
            let is_init = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .is_some_and(|n| n == "__init__");
            if is_init && let Some(func_body) = child.child_by_field_name("body") {
                collect_self_assignments(func_body, source, &mut attrs);
            }
        }
    }
    attrs.sort();
    attrs.dedup();
    attrs
}

fn collect_self_assignments(node: Node<'_>, source: &[u8], out: &mut Vec<String>) {
    if node.kind() == "assignment" {
        let lhs = node.child_by_field_name("left");
        if let Some(lhs_node) = lhs
            && lhs_node.kind() == "attribute"
        {
            let obj = lhs_node.child_by_field_name("object");
            let attr = lhs_node.child_by_field_name("attribute");
            if let (Some(obj_node), Some(attr_node)) = (obj, attr)
                && obj_node.utf8_text(source).unwrap_or("") == "self"
                && let Ok(attr_name) = attr_node.utf8_text(source)
            {
                out.push(attr_name.to_string());
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_self_assignments(child, source, out);
    }
}

fn extract_method_names(body: Node<'_>, source: &[u8]) -> Vec<String> {
    let mut methods = Vec::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "function_definition"
            && let Some(name_node) = child.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(source)
        {
            let is_dunder = name.starts_with("__") && name.ends_with("__");
            if !is_dunder {
                methods.push(name.to_string());
            }
        }
    }
    methods
}

fn collect_identifiers(node: Node<'_>, source: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    collect_identifiers_rec(node, source, &mut out);
    out
}

fn collect_identifiers_rec(node: Node<'_>, source: &[u8], out: &mut Vec<String>) {
    if node.kind() == "identifier"
        && let Ok(text) = node.utf8_text(source)
    {
        out.push(text.to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers_rec(child, source, out);
    }
}

fn resolve_class_deps(
    rc: RawClass,
    all_class_names: &HashSet<String>,
    qualified_class_set: &HashSet<String>,
) -> ClassDef {
    // Reverse lookup: short class name → all qualified names that carry it.
    // Used in the post-pass to qualify bare short-name deps unambiguously.
    let mut short_to_qualified: HashMap<&str, Vec<&str>> = HashMap::new();
    for qname in qualified_class_set {
        if let Some(short) = qname.rsplit('.').next() {
            short_to_qualified
                .entry(short)
                .or_default()
                .push(qname.as_str());
        }
    }

    let raw_deps: Vec<String> = rc
        .referenced_names
        .iter()
        .filter(|n| *n != &rc.name && all_class_names.contains(*n))
        .map(|n| {
            if let Some(src_module) = rc.imported_names.get(n) {
                let qualified = format!("{src_module}.{n}");
                if qualified_class_set.contains(&qualified) {
                    return qualified;
                }
            }
            n.clone()
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // Post-pass: qualify any remaining bare short names. Drop ambiguous ones.
    let mut class_deps: Vec<String> = raw_deps
        .into_iter()
        .filter_map(|dep| {
            if qualified_class_set.contains(&dep) {
                return Some(dep);
            }
            match short_to_qualified.get(dep.as_str()) {
                Some(candidates) if candidates.len() == 1 => Some(candidates[0].to_string()),
                _ => None,
            }
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    class_deps.sort();

    ClassDef {
        module: rc.module,
        name: rc.name,
        bases: rc.bases,
        attributes: rc.attributes,
        methods: rc.methods,
        class_deps,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> ParsedFile {
        ParsedFile::parse(src.as_bytes()).expect("parse failed")
    }

    #[test]
    fn test_extract_module_names_import_should_return_module() {
        assert_eq!(
            extract_module_names("import foo.bar", "pkg.mod"),
            vec!["foo.bar"]
        );
    }

    #[test]
    fn test_extract_module_names_from_import_should_return_module() {
        assert_eq!(
            extract_module_names("from myapp.domain import Order", "myapp.views"),
            vec!["myapp.domain"]
        );
    }

    #[test]
    fn test_extract_module_names_single_dot_relative_should_resolve_to_sibling() {
        // `from .customer import Customer` in `src.order` → `src.customer`
        assert_eq!(
            extract_module_names("from .customer import Customer", "src.order"),
            vec!["src.customer"]
        );
    }

    #[test]
    fn test_extract_module_names_single_dot_bare_relative_should_resolve_to_package() {
        // `from . import utils` in `src.order` → `src`
        assert_eq!(
            extract_module_names("from . import utils", "src.order"),
            vec!["src"]
        );
    }

    #[test]
    fn test_extract_module_names_double_dot_relative_should_resolve_to_grandparent() {
        // `from ..domain import Order` in `myapp.views.json` → `myapp.domain`
        assert_eq!(
            extract_module_names("from ..domain import Order", "myapp.views.json"),
            vec!["myapp.domain"]
        );
    }

    #[test]
    fn test_extract_module_names_relative_too_many_dots_should_return_empty() {
        // More dots than package depth — unresolvable
        assert!(extract_module_names("from ...x import Y", "src.order").is_empty());
    }

    #[test]
    fn test_collect_module_deps_should_emit_edges() {
        let src = "import myapp.domain\nfrom myapp.usecases import CreateOrder\n";
        let parsed = parse(src);
        let mut deps = Vec::new();
        collect_module_deps(&parsed, "myapp.views", &mut deps);
        let actual: Vec<(String, String)> = deps
            .into_iter()
            .map(|d| (d.from.to_string(), d.to.to_string()))
            .collect();
        assert_eq!(
            actual,
            vec![
                ("myapp.views".to_string(), "myapp.domain".to_string()),
                ("myapp.views".to_string(), "myapp.usecases".to_string()),
            ]
        );
    }

    #[test]
    fn test_collect_module_deps_relative_import_should_resolve_to_sibling_module() {
        let src = "from .customer import Customer\n";
        let parsed = parse(src);
        let mut deps = Vec::new();
        collect_module_deps(&parsed, "src.order", &mut deps);
        let actual: Vec<(String, String)> = deps
            .into_iter()
            .map(|d| (d.from.to_string(), d.to.to_string()))
            .collect();
        assert_eq!(
            actual,
            vec![("src.order".to_string(), "src.customer".to_string())]
        );
    }

    #[test]
    fn test_extract_bases_should_return_base_class_names() {
        let src = "class Order(Base, Mixin):\n    pass\n";
        let parsed = parse(src);
        let root = parsed.root();
        let class_node = root.child(0).expect("class node");
        let actual = extract_bases(class_node, &parsed.source);
        assert_eq!(actual, vec!["Base", "Mixin"]);
    }

    #[test]
    fn test_extract_self_attributes_should_return_init_assignments() {
        let src = "class Order:\n    def __init__(self, id):\n        self.id = id\n        self.status = 'pending'\n";
        let parsed = parse(src);
        let root = parsed.root();
        let class_node = root.child(0).expect("class node");
        let body = class_node.child_by_field_name("body").expect("body");
        let actual = extract_self_attributes(body, &parsed.source);
        assert_eq!(actual, vec!["id", "status"]);
    }

    #[test]
    fn test_extract_method_names_should_skip_dunders() {
        let src = "class Order:\n    def __init__(self): pass\n    def cancel(self): pass\n    def approve(self): pass\n";
        let parsed = parse(src);
        let root = parsed.root();
        let class_node = root.child(0).expect("class node");
        let body = class_node.child_by_field_name("body").expect("body");
        let actual = extract_method_names(body, &parsed.source);
        assert_eq!(actual, vec!["cancel", "approve"]);
    }

    #[test]
    fn test_resolve_class_deps_should_reference_known_classes() {
        let all = HashSet::from(["Customer".to_string(), "Product".to_string()]);
        let qualified = HashSet::from([
            "myapp.domain.Customer".to_string(),
            "myapp.domain.Product".to_string(),
        ]);
        let rc = RawClass {
            module: "myapp.domain".to_string(),
            name: "Order".to_string(),
            bases: vec![],
            attributes: vec![],
            methods: vec![],
            referenced_names: vec![
                "Customer".to_string(),
                "Product".to_string(),
                "str".to_string(),
                "Order".to_string(),
            ],
            imported_names: HashMap::new(),
        };
        let def = resolve_class_deps(rc, &all, &qualified);
        assert_eq!(
            def.class_deps,
            vec!["myapp.domain.Customer", "myapp.domain.Product"]
        );
    }

    #[test]
    fn test_resolve_class_deps_same_module_dep_should_be_qualified() {
        let all = HashSet::from(["Customer".to_string()]);
        let qualified = HashSet::from(["myapp.domain.Customer".to_string()]);
        let rc = RawClass {
            module: "myapp.domain".to_string(),
            name: "Order".to_string(),
            bases: vec![],
            attributes: vec![],
            methods: vec![],
            referenced_names: vec!["Customer".to_string()],
            imported_names: HashMap::new(),
        };
        let def = resolve_class_deps(rc, &all, &qualified);
        assert_eq!(def.class_deps, vec!["myapp.domain.Customer"]);
    }

    #[test]
    fn test_resolve_class_deps_imported_dep_stays_qualified() {
        let all = HashSet::from(["Customer".to_string()]);
        let qualified = HashSet::from(["myapp.customers.Customer".to_string()]);
        let mut imported = HashMap::new();
        imported.insert("Customer".to_string(), "myapp.customers".to_string());
        let rc = RawClass {
            module: "myapp.domain".to_string(),
            name: "Order".to_string(),
            bases: vec![],
            attributes: vec![],
            methods: vec![],
            referenced_names: vec!["Customer".to_string()],
            imported_names: imported,
        };
        let def = resolve_class_deps(rc, &all, &qualified);
        assert_eq!(def.class_deps, vec!["myapp.customers.Customer"]);
    }

    #[test]
    fn test_resolve_class_deps_ambiguous_short_name_is_dropped() {
        let all = HashSet::from(["Base".to_string()]);
        let qualified = HashSet::from([
            "myapp.domain.Base".to_string(),
            "myapp.infra.Base".to_string(),
        ]);
        let rc = RawClass {
            module: "myapp.service".to_string(),
            name: "Service".to_string(),
            bases: vec![],
            attributes: vec![],
            methods: vec![],
            referenced_names: vec!["Base".to_string()],
            imported_names: HashMap::new(),
        };
        let def = resolve_class_deps(rc, &all, &qualified);
        assert!(def.class_deps.is_empty());
    }

    #[test]
    fn test_resolve_class_deps_self_reference_excluded() {
        let all = HashSet::from(["Order".to_string()]);
        let qualified = HashSet::from(["myapp.domain.Order".to_string()]);
        let rc = RawClass {
            module: "myapp.domain".to_string(),
            name: "Order".to_string(),
            bases: vec![],
            attributes: vec![],
            methods: vec![],
            referenced_names: vec!["Order".to_string()],
            imported_names: HashMap::new(),
        };
        let def = resolve_class_deps(rc, &all, &qualified);
        assert!(def.class_deps.is_empty());
    }

    // --- Integration tests on the public extract() function ---

    fn write_pkg(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmp dir");
        for (name, src) in files {
            std::fs::write(dir.path().join(name), src).expect("write");
        }
        dir
    }

    #[test]
    fn test_extract_single_class_file_should_return_one_class_def() {
        let pkg = write_pkg(&[("customer.py", "class Customer:\n    pass\n")]);
        let (_, classes) = extract(pkg.path()).unwrap();
        assert_eq!(classes.len(), 1);
        assert!(classes[0].class_deps.is_empty());
    }

    #[test]
    fn test_extract_two_classes_same_file_should_produce_qualified_dep() {
        let pkg = write_pkg(&[(
            "domain.py",
            "class Customer:\n    pass\nclass Order:\n    def run(self, c: Customer): pass\n",
        )]);
        let (_, classes) = extract(pkg.path()).unwrap();
        let order = classes.iter().find(|c| c.name == "Order").unwrap();
        assert_eq!(order.class_deps.len(), 1);
        assert!(order.class_deps[0].ends_with(".Customer"));
    }

    #[test]
    fn test_extract_cross_module_dep_via_import_should_be_qualified() {
        let pkg = write_pkg(&[
            ("customer.py", "class Customer:\n    pass\n"),
            (
                "order.py",
                "from .customer import Customer\nclass Order:\n    def __init__(self, c: Customer): pass\n",
            ),
        ]);
        let (_, classes) = extract(pkg.path()).unwrap();
        let order = classes.iter().find(|c| c.name == "Order").unwrap();
        assert_eq!(order.class_deps.len(), 1);
        assert!(order.class_deps[0].ends_with("customer.Customer"));
    }

    #[test]
    fn test_extract_cross_module_no_import_ambiguous_should_drop_dep() {
        // Two modules both define Foo — Order references "Foo" without importing it.
        // Ambiguous: dep must be dropped.
        let pkg = write_pkg(&[
            ("a.py", "class Foo:\n    pass\n"),
            ("b.py", "class Foo:\n    pass\n"),
            (
                "order.py",
                "class Order:\n    def run(self, x: Foo): pass\n",
            ),
        ]);
        let (_, classes) = extract(pkg.path()).unwrap();
        let order = classes.iter().find(|c| c.name == "Order").unwrap();
        assert!(order.class_deps.is_empty());
    }

    #[test]
    fn test_extract_self_reference_should_be_excluded() {
        let pkg = write_pkg(&[(
            "domain.py",
            "class Order:\n    def clone(self) -> 'Order': pass\n",
        )]);
        let (_, classes) = extract(pkg.path()).unwrap();
        let order = classes.iter().find(|c| c.name == "Order").unwrap();
        assert!(order.class_deps.is_empty());
    }

    #[test]
    fn test_extract_stdlib_base_class_not_in_graph_should_produce_no_dep() {
        let pkg = write_pkg(&[(
            "repo.py",
            "from typing import Protocol\nclass Repo(Protocol):\n    pass\n",
        )]);
        let (_, classes) = extract(pkg.path()).unwrap();
        let repo = classes.iter().find(|c| c.name == "Repo").unwrap();
        assert!(repo.class_deps.is_empty());
    }

    #[test]
    fn test_extract_module_deps_should_reflect_import_statements() {
        let pkg = write_pkg(&[
            ("customer.py", "class Customer:\n    pass\n"),
            (
                "order.py",
                "from .customer import Customer\nclass Order:\n    pass\n",
            ),
        ]);
        let (module_deps, _) = extract(pkg.path()).unwrap();
        let has_edge = module_deps
            .iter()
            .any(|d| d.from.contains("order") && d.to.contains("customer"));
        assert!(has_edge);
    }

    #[test]
    fn test_extract_multiple_files_should_collect_all_classes() {
        let pkg = write_pkg(&[
            ("customer.py", "class Customer:\n    pass\n"),
            ("order.py", "class Order:\n    pass\n"),
            ("service.py", "class Service:\n    pass\n"),
        ]);
        let (_, classes) = extract(pkg.path()).unwrap();
        assert_eq!(classes.len(), 3);
    }

    #[test]
    fn test_extract_empty_directory_should_return_empty_vecs() {
        let pkg = write_pkg(&[]);
        let (module_deps, classes) = extract(pkg.path()).unwrap();
        assert!(module_deps.is_empty());
        assert!(classes.is_empty());
    }
}
