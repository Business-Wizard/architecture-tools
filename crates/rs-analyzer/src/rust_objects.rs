use std::collections::{HashMap, HashSet};
use std::path::Path;

use ignore::Walk;
use lang_core::ClassDef;
use tree_sitter::{Node, Parser};

use crate::error::InspectorError;
use crate::rust_imports::path_to_module_name;

pub fn extract(root: &Path) -> Result<Vec<ClassDef>, InspectorError> {
    let rs_files: Vec<_> = Walk::new(root)
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
        .map(ignore::DirEntry::into_path)
        .collect();

    let mut raw_items: Vec<RawItem> = Vec::new();

    for file in &rs_files {
        let source = std::fs::read(file)?;
        let module_name = path_to_module_name(file, root);
        let Some(tree) = parse_source(&source) else {
            continue;
        };
        collect_raw_items(tree.root_node(), &source, &module_name, &mut raw_items);
    }

    let qualified_set: HashSet<String> = raw_items
        .iter()
        .map(|i| format!("{}.{}", i.module, i.name))
        .collect();
    let short_names: HashSet<String> = raw_items.iter().map(|i| i.name.clone()).collect();

    // Build reverse lookup: short name → all qualified names.
    let mut short_to_qualified: HashMap<&str, Vec<&str>> = HashMap::new();
    for qname in &qualified_set {
        if let Some(short) = qname.rsplit('.').next() {
            short_to_qualified
                .entry(short)
                .or_default()
                .push(qname.as_str());
        }
    }

    let defs = raw_items
        .into_iter()
        .map(|ri| resolve_deps(ri, &short_names, &short_to_qualified))
        .collect();

    Ok(defs)
}

#[derive(Debug)]
struct RawItem {
    module: String,
    name: String,
    kind: ItemKind,
    /// Trait names this item is `impl`-ed for (bases).
    trait_impls: Vec<String>,
    /// Raw identifiers referenced in field types / variants / fn signatures.
    referenced_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Struct,
    Enum,
    Trait,
}

fn parse_source(source: &[u8]) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .ok()?;
    parser.parse(source, None)
}

fn collect_raw_items(root: Node<'_>, source: &[u8], module: &str, out: &mut Vec<RawItem>) {
    // First pass: collect all struct/enum/trait item names in this file.
    let local_names: HashSet<String> = root
        .children(&mut root.walk())
        .filter_map(|n| {
            if matches!(n.kind(), "struct_item" | "enum_item" | "trait_item") {
                item_name(n, source)
            } else {
                None
            }
        })
        .collect();

    // Collect impl blocks: maps type_name → Vec<trait_name>.
    let mut trait_impls: HashMap<String, Vec<String>> = HashMap::new();
    for child in root.children(&mut root.walk()) {
        if child.kind() == "impl_item"
            && let Some((type_name, trait_name)) = extract_impl_trait(child, source)
        {
            trait_impls.entry(type_name).or_default().push(trait_name);
        }
    }

    for child in root.children(&mut root.walk()) {
        let kind = match child.kind() {
            "struct_item" => ItemKind::Struct,
            "enum_item" => ItemKind::Enum,
            "trait_item" => ItemKind::Trait,
            _ => continue,
        };

        let Some(name) = item_name(child, source) else {
            continue;
        };

        let referenced_names = collect_type_identifiers(child, source, &local_names);
        let impls = trait_impls.get(&name).cloned().unwrap_or_default();

        out.push(RawItem {
            module: module.to_string(),
            name,
            kind,
            trait_impls: impls,
            referenced_names,
        });
    }
}

fn item_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)
}

/// Returns `(type_name, trait_name)` for `impl TraitName for TypeName` blocks.
/// Returns `None` for plain `impl TypeName` (no trait).
fn extract_impl_trait(node: Node<'_>, source: &[u8]) -> Option<(String, String)> {
    // The tree-sitter Rust grammar represents `impl Trait for Type` as:
    //   impl_item
    //     type: scoped_identifier | identifier  ← this is the TYPE when no trait
    //     trait: scoped_identifier | identifier ← present only for `impl Trait for T`
    // When a trait is present, `trait` field = trait name, `type` field = implementing type.
    let trait_node = node.child_by_field_name("trait")?;
    let type_node = node.child_by_field_name("type")?;
    let trait_name = short_name(trait_node.utf8_text(source).ok()?);
    let type_name = short_name(type_node.utf8_text(source).ok()?);
    Some((type_name, trait_name))
}

/// Extract the last `::` component (short name) from a possibly-qualified path.
fn short_name(s: &str) -> String {
    s.rsplit("::").next().unwrap_or(s).trim().to_string()
}

/// Extract the crate prefix (up to and including "src") from a dotted module path.
/// E.g. `"crates.awt.src.graph.object_graph"` → `"crates.awt.src"`
fn crate_prefix_of(module: &str) -> String {
    let mut acc = String::new();
    for part in module.split('.') {
        if !acc.is_empty() {
            acc.push('.');
        }
        acc.push_str(part);
        if part == "src" {
            return acc;
        }
    }
    module.split('.').next().unwrap_or("").to_owned()
}

/// Resolve a short type name to its qualified name, preferring same-crate candidates.
/// Returns the unique candidate if exactly one exists.
/// If multiple candidates exist, returns one from the same crate (by prefix).
/// Returns None if ambiguous (multiple same-crate candidates or no candidates).
fn resolve_candidate(
    short: &str,
    item_module: &str,
    short_to_qualified: &HashMap<&str, Vec<&str>>,
) -> Option<String> {
    let candidates = short_to_qualified.get(short)?;
    if candidates.len() == 1 {
        return Some(candidates[0].to_string());
    }
    let prefix = crate_prefix_of(item_module);
    let same_crate: Vec<&str> = candidates
        .iter()
        .copied()
        .filter(|q| q.starts_with(prefix.as_str()))
        .collect();
    if same_crate.len() == 1 {
        Some(same_crate[0].to_string())
    } else {
        None
    }
}

/// Walk the item subtree collecting identifiers that look like type names
/// (`PascalCase`) and are in the file's local name set. Skips the item's own name.
fn collect_type_identifiers(
    item: Node<'_>,
    source: &[u8],
    local_names: &HashSet<String>,
) -> Vec<String> {
    let own_name = item_name(item, source).unwrap_or_default();
    let mut out = Vec::new();
    collect_type_ids_rec(item, source, &own_name, local_names, &mut out);
    out
}

fn collect_type_ids_rec(
    node: Node<'_>,
    source: &[u8],
    own_name: &str,
    local_names: &HashSet<String>,
    out: &mut Vec<String>,
) {
    // Collect `type_identifier` nodes (PascalCase type names in the grammar).
    if node.kind() == "type_identifier"
        && let Ok(text) = node.utf8_text(source)
        && text != own_name
        && local_names.contains(text)
    {
        out.push(text.to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_type_ids_rec(child, source, own_name, local_names, out);
    }
}

fn resolve_deps(
    ri: RawItem,
    short_names: &HashSet<String>,
    short_to_qualified: &HashMap<&str, Vec<&str>>,
) -> ClassDef {
    // Qualify referenced names using the same unambiguous-single-match strategy.
    let mut class_deps: Vec<String> = ri
        .referenced_names
        .iter()
        .filter(|n| *n != &ri.name && short_names.contains(*n))
        .filter_map(|n| resolve_candidate(n, &ri.module, short_to_qualified))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    class_deps.sort();

    // bases: trait names for structs/enums, empty for traits themselves.
    // Qualify trait names through resolve_candidate so object_graph can find them in node_map.
    let bases: Vec<String> = ri
        .trait_impls
        .into_iter()
        .filter_map(|t| resolve_candidate(&t, &ri.module, short_to_qualified))
        .collect();

    let kind_marker = match ri.kind {
        ItemKind::Trait => "Trait",
        ItemKind::Struct | ItemKind::Enum => "",
    };

    // For traits, mark them as "traitlike" via the bases convention.
    let mut effective_bases = bases;
    if ri.kind == ItemKind::Trait {
        effective_bases.insert(0, kind_marker.to_string());
    }

    ClassDef {
        module: ri.module,
        name: ri.name,
        bases: effective_bases,
        attributes: vec![],
        methods: vec![],
        class_deps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> tree_sitter::Tree {
        parse_source(src.as_bytes()).expect("parse failed")
    }

    fn write_pkg(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmp dir");
        for (name, src) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create dir");
            }
            std::fs::write(path, src).expect("write");
        }
        dir
    }

    #[test]
    fn test_item_name_struct_should_return_name() {
        let src = "struct Order { id: u32 }";
        let tree = parse(src);
        let node = tree.root_node().child(0).unwrap();
        assert_eq!(item_name(node, src.as_bytes()).as_deref(), Some("Order"));
    }

    #[test]
    fn test_collect_raw_items_should_find_struct_enum_trait() {
        let src = "struct Foo;\nenum Bar { A }\ntrait Baz {}";
        let tree = parse(src);
        let mut items = Vec::new();
        collect_raw_items(tree.root_node(), src.as_bytes(), "mod", &mut items);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Foo"));
        assert!(names.contains(&"Bar"));
        assert!(names.contains(&"Baz"));
    }

    #[test]
    fn test_collect_raw_items_should_set_kind_correctly() {
        let src = "struct S;\nenum E {}\ntrait T {}";
        let tree = parse(src);
        let mut items = Vec::new();
        collect_raw_items(tree.root_node(), src.as_bytes(), "mod", &mut items);
        let s = items.iter().find(|i| i.name == "S").unwrap();
        let e = items.iter().find(|i| i.name == "E").unwrap();
        let t = items.iter().find(|i| i.name == "T").unwrap();
        assert_eq!(s.kind, ItemKind::Struct);
        assert_eq!(e.kind, ItemKind::Enum);
        assert_eq!(t.kind, ItemKind::Trait);
    }

    #[test]
    fn test_extract_impl_trait_should_return_trait_and_type() {
        let src = "impl MyTrait for MyStruct {}";
        let tree = parse(src);
        let impl_node = tree.root_node().child(0).unwrap();
        let actual = extract_impl_trait(impl_node, src.as_bytes());
        assert_eq!(
            actual,
            Some(("MyStruct".to_string(), "MyTrait".to_string()))
        );
    }

    #[test]
    fn test_extract_impl_no_trait_should_return_none() {
        let src = "impl MyStruct { fn new() -> Self { MyStruct } }";
        let tree = parse(src);
        let impl_node = tree.root_node().child(0).unwrap();
        let actual = extract_impl_trait(impl_node, src.as_bytes());
        assert_eq!(actual, None);
    }

    #[test]
    fn test_extract_single_struct_should_return_one_class_def() {
        let pkg = write_pkg(&[("order.rs", "pub struct Order { pub id: u32 }\n")]);
        let defs = extract(pkg.path()).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "Order");
    }

    #[test]
    fn test_extract_struct_using_other_struct_should_produce_qualified_dep() {
        let pkg = write_pkg(&[(
            "domain.rs",
            "pub struct Customer { pub id: u32 }\npub struct Order { pub customer: Customer }\n",
        )]);
        let defs = extract(pkg.path()).unwrap();
        let order = defs.iter().find(|d| d.name == "Order").unwrap();
        assert_eq!(order.class_deps.len(), 1);
        assert!(order.class_deps[0].ends_with(".Customer"));
    }

    #[test]
    fn test_extract_trait_should_have_trait_kind_marker_in_bases() {
        let pkg = write_pkg(&[("repo.rs", "pub trait Repository {}\n")]);
        let defs = extract(pkg.path()).unwrap();
        let repo = defs.iter().find(|d| d.name == "Repository").unwrap();
        assert!(repo.bases.contains(&"Trait".to_string()));
    }

    #[test]
    fn test_extract_impl_trait_for_struct_should_set_bases() {
        let pkg = write_pkg(&[(
            "domain.rs",
            "pub trait Repo {}\npub struct SqlRepo;\nimpl Repo for SqlRepo {}\n",
        )]);
        let defs = extract(pkg.path()).unwrap();
        let sql_repo = defs.iter().find(|d| d.name == "SqlRepo").unwrap();
        assert!(sql_repo.bases.iter().any(|b| b.ends_with(".Repo")));
    }

    #[test]
    fn test_extract_multiple_files_should_collect_all_items() {
        let pkg = write_pkg(&[
            ("order.rs", "pub struct Order;\n"),
            ("customer.rs", "pub struct Customer;\n"),
            ("service.rs", "pub trait Service {}\n"),
        ]);
        let defs = extract(pkg.path()).unwrap();
        assert_eq!(defs.len(), 3);
    }

    #[test]
    fn test_extract_empty_directory_should_return_empty() {
        let pkg = write_pkg(&[]);
        let defs = extract(pkg.path()).unwrap();
        assert!(defs.is_empty());
    }

    #[test]
    fn test_extract_self_reference_in_struct_should_be_excluded() {
        let pkg = write_pkg(&[("list.rs", "pub struct Node { pub next: Option<Node> }\n")]);
        let defs = extract(pkg.path()).unwrap();
        let node = defs.iter().find(|d| d.name == "Node").unwrap();
        assert!(node.class_deps.is_empty());
    }

    #[test]
    fn test_crate_prefix_of_with_src_segment_should_return_prefix_up_to_src() {
        assert_eq!(
            crate_prefix_of("crates.awt.src.graph.object_graph"),
            "crates.awt.src"
        );
        assert_eq!(crate_prefix_of("src.model"), "src");
        assert_eq!(crate_prefix_of("mylib"), "mylib");
    }

    #[test]
    fn test_resolve_deps_same_crate_wins_when_name_exists_in_two_crates_should_produce_edge() {
        let pkg = write_pkg(&[
            (
                "crate_a/src/types.rs",
                "pub enum ObjectKind { A }\npub struct ObjectNode { pub kind: ObjectKind }\n",
            ),
            ("crate_b/src/types.rs", "pub enum ObjectKind { B }\n"),
        ]);
        let defs = extract(pkg.path()).unwrap();
        let node = defs.iter().find(|d| d.name == "ObjectNode").unwrap();
        assert_eq!(node.class_deps.len(), 1);
        assert!(node.class_deps[0].contains("crate_a"));
    }

    #[test]
    fn test_resolve_deps_cross_crate_ambiguous_with_no_local_winner_should_drop_dep() {
        let pkg = write_pkg(&[
            ("crate_a/src/types.rs", "pub struct Widget;\n"),
            ("crate_b/src/types.rs", "pub struct Widget;\n"),
            (
                "crate_c/src/service.rs",
                "pub struct Service { pub w: Widget }\n",
            ),
        ]);
        let defs = extract(pkg.path()).unwrap();
        let svc = defs.iter().find(|d| d.name == "Service").unwrap();
        assert!(svc.class_deps.is_empty());
    }

    #[test]
    fn test_extract_trait_impl_cross_file_should_produce_qualified_base() {
        let pkg = write_pkg(&[
            ("traits.rs", "pub trait InstabilityStrategy {}\n"),
            (
                "impls.rs",
                "pub struct ConcreteStrategy;\nimpl InstabilityStrategy for ConcreteStrategy {}\n",
            ),
        ]);
        let defs = extract(pkg.path()).unwrap();
        let concrete = defs.iter().find(|d| d.name == "ConcreteStrategy").unwrap();
        assert!(
            concrete
                .bases
                .iter()
                .any(|b| b.ends_with(".InstabilityStrategy"))
        );
    }
}
