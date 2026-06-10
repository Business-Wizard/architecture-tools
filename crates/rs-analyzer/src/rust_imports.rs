use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ignore::Walk;
use lang_core::ModuleDep;
use tree_sitter::{Node, Parser};

use crate::error::InspectorError;

pub fn extract(root: &Path) -> Result<Vec<ModuleDep>, InspectorError> {
    let rs_files = collect_rust_files(root);
    let mut deps = Vec::new();
    let crate_map = build_crate_map(root);

    for file in &rs_files {
        let source = std::fs::read(file)?;
        let Some(tree) = parse_source(&source) else {
            continue;
        };
        let module_name = path_to_module_name(file, root);
        let crate_root_prefix = crate_root_for(file, root);
        collect_use_deps(
            tree.root_node(),
            &source,
            &module_name,
            &crate_root_prefix,
            &crate_map,
            &mut deps,
        );
    }

    Ok(deps)
}

fn collect_rust_files(root: &Path) -> Vec<PathBuf> {
    Walk::new(root)
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
        .map(ignore::DirEntry::into_path)
        .collect()
}

fn build_crate_map(root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();

    Walk::new(root)
        .flatten()
        .filter(|e| e.file_name() == "Cargo.toml")
        .for_each(|entry| {
            let cargo_path = entry.path();
            if let Ok(content) = std::fs::read_to_string(cargo_path)
                && let Some(name) = extract_crate_name(&content)
            {
                let crate_dir = cargo_path.parent().unwrap_or(root);
                let normalized_name = name.replace('-', "_");

                let lib_path = crate_dir.join("src/lib.rs");
                let main_path = crate_dir.join("src/main.rs");

                if lib_path.exists() {
                    let module_name = path_to_module_name(&lib_path, root);
                    map.insert(normalized_name, module_name);
                } else if main_path.exists() {
                    let module_name = path_to_module_name(&main_path, root);
                    map.insert(normalized_name, module_name);
                }
            }
        });

    map
}

fn extract_crate_name(cargo_content: &str) -> Option<String> {
    for line in cargo_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name")
            && trimmed.contains('=')
            && let Some(quoted) = trimmed.split('=').nth(1)
        {
            let name = quoted.trim().trim_matches('"').trim_matches('\'').trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

pub(crate) fn path_to_module_name(file: &Path, root: &Path) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file);
    let s = rel.to_string_lossy();
    let without_ext = s.strip_suffix(".rs").unwrap_or(&s);
    let normalized = without_ext
        .strip_suffix("/lib")
        .or_else(|| without_ext.strip_suffix("/main"))
        .or_else(|| without_ext.strip_suffix("/mod"))
        .unwrap_or(without_ext);
    normalized.replace(['/', '\\'], ".")
}

// Returns the dotted prefix anchoring `crate::` paths for a given file.
// Walks up from the file to find a `src/` ancestor within root, then uses
// the dotted path up to and including that `src` component.
pub(crate) fn crate_root_for(file: &Path, root: &Path) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file);
    let components: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Find last "src" component — that's the crate root boundary.
    if let Some(src_idx) = components.iter().rposition(|&c| c == "src") {
        let prefix_parts = &components[..=src_idx];
        return prefix_parts.join(".");
    }

    // Fallback: use the first component (crate directory name).
    components.first().copied().unwrap_or("").to_owned()
}

fn parse_source(source: &[u8]) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .ok()?;
    parser.parse(source, None)
}

fn collect_use_deps(
    root: Node<'_>,
    source: &[u8],
    module_name: &str,
    crate_root: &str,
    crate_map: &HashMap<String, String>,
    out: &mut Vec<ModuleDep>,
) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "use_declaration" => {
                if let Some(use_tree) = child.child_by_field_name("argument") {
                    expand_use_tree(
                        use_tree,
                        source,
                        module_name,
                        crate_root,
                        &[],
                        crate_map,
                        out,
                    );
                }
            }
            "mod_item" => {
                collect_mod_dep(child, source, module_name, out);
            }
            _ => {}
        }
    }
}

fn collect_mod_dep(node: Node<'_>, source: &[u8], module_name: &str, out: &mut Vec<ModuleDep>) {
    let has_body = node
        .children(&mut node.walk())
        .any(|c| c.kind() == "declaration_list");
    if has_body {
        return;
    }
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Ok(name) = name_node.utf8_text(source) else {
        return;
    };
    let to = format!("{module_name}.{name}");
    out.push(ModuleDep {
        from: module_name.into(),
        to: to.into(),
    });
}

// Recursively expand a use_tree node, accumulating path components.
fn expand_use_tree(
    node: Node<'_>,
    source: &[u8],
    module_name: &str,
    crate_root: &str,
    prefix: &[String],
    crate_map: &HashMap<String, String>,
    out: &mut Vec<ModuleDep>,
) {
    match node.kind() {
        "scoped_use_list" => {
            // e.g. `crate::graph::{coupling_graph, metrics}`
            // child 0 = path, child 1 = "::", child 2 = use_list
            let path_node = node.child(0);
            let list_node = node
                .children(&mut node.walk())
                .find(|n| n.kind() == "use_list");

            let mut new_prefix = prefix.to_vec();
            if let Some(p) = path_node
                && let Ok(text) = p.utf8_text(source)
            {
                new_prefix.extend(text.split("::").map(str::to_owned));
            }
            if let Some(list) = list_node {
                let mut cursor = list.walk();
                for item in list.children(&mut cursor) {
                    if item.kind() != "," && item.kind() != "{" && item.kind() != "}" {
                        expand_use_tree(
                            item,
                            source,
                            module_name,
                            crate_root,
                            &new_prefix,
                            crate_map,
                            out,
                        );
                    }
                }
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for item in node.children(&mut cursor) {
                if item.kind() != "," && item.kind() != "{" && item.kind() != "}" {
                    expand_use_tree(
                        item,
                        source,
                        module_name,
                        crate_root,
                        prefix,
                        crate_map,
                        out,
                    );
                }
            }
        }
        "use_as_clause" => {
            // `X as Y` — extract the path before `as`, ignore the alias.
            if let Some(path_node) = node.child(0) {
                expand_use_tree(
                    path_node,
                    source,
                    module_name,
                    crate_root,
                    prefix,
                    crate_map,
                    out,
                );
            }
        }
        "scoped_identifier" | "identifier" | "use_wildcard" => {
            let Ok(text) = node.utf8_text(source) else {
                return;
            };
            // Strip trailing `::*` from wildcards.
            let text = text.trim_end_matches("::*");
            let mut parts = prefix.to_vec();
            parts.extend(text.split("::").map(str::to_owned));
            emit_dep(&parts, module_name, crate_root, crate_map, out);
        }
        _ => {}
    }
}

// Strip trailing path components that name types/traits/consts (PascalCase or `*`),
// keeping only the module path. `use crate::graph::coupling_graph::GraphIndex` → dep on
// `crate::graph::coupling_graph`, not `crate::graph::coupling_graph::GraphIndex`.
fn module_parts(parts: &[String]) -> &[String] {
    let end = parts
        .iter()
        .rposition(|p| p.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') && p != "*")
        .map_or(0, |i| i + 1);
    &parts[..end]
}

fn emit_dep(
    parts: &[String],
    module_name: &str,
    crate_root: &str,
    crate_map: &HashMap<String, String>,
    out: &mut Vec<ModuleDep>,
) {
    let Some(first) = parts.first() else {
        return;
    };

    let mod_parts = module_parts(parts);

    let to = match first.as_str() {
        "crate" => {
            let rest: Vec<&str> = mod_parts[1..].iter().map(String::as_str).collect();
            if rest.is_empty() {
                crate_root.to_owned()
            } else {
                format!("{}.{}", crate_root, rest.join("."))
            }
        }
        "super" => {
            let parent = module_name.rsplit_once('.').map_or("", |(p, _)| p);
            let rest: Vec<&str> = mod_parts[1..].iter().map(String::as_str).collect();
            if rest.is_empty() {
                parent.to_owned()
            } else {
                format!("{}.{}", parent, rest.join("."))
            }
        }
        _ => {
            if let Some(crate_module) = crate_map.get(first.as_str()) {
                let rest: Vec<&str> = mod_parts[1..].iter().map(String::as_str).collect();
                if rest.is_empty() {
                    crate_module.clone()
                } else {
                    format!("{}.{}", crate_module, rest.join("."))
                }
            } else {
                return;
            }
        }
    };

    if to.is_empty() || to == module_name {
        return;
    }

    out.push(ModuleDep {
        from: module_name.into(),
        to: to.into(),
    });
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn test_path_to_module_name_regular_file_should_strip_rs_extension() {
        let root = Path::new("/project");
        let file = Path::new("/project/src/graph/coupling_graph.rs");
        let actual = path_to_module_name(file, root);
        assert_eq!(actual, "src.graph.coupling_graph");
    }

    #[test]
    fn test_path_to_module_name_lib_rs_should_resolve_to_src() {
        let root = Path::new("/project");
        let file = Path::new("/project/src/lib.rs");
        let actual = path_to_module_name(file, root);
        assert_eq!(actual, "src");
    }

    #[test]
    fn test_path_to_module_name_main_rs_should_resolve_to_src() {
        let root = Path::new("/project");
        let file = Path::new("/project/src/main.rs");
        let actual = path_to_module_name(file, root);
        assert_eq!(actual, "src");
    }

    #[test]
    fn test_crate_root_for_with_src_segment_should_return_up_to_src() {
        let root = Path::new("/workspace");
        let file = Path::new("/workspace/crates/awt/src/cli.rs");
        let actual = crate_root_for(file, root);
        assert_eq!(actual, "crates.awt.src");
    }

    #[test]
    fn test_emit_dep_crate_path_should_prepend_crate_root() {
        let mut out = Vec::new();
        let parts: Vec<String> = ["crate", "graph", "coupling_graph"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let crate_map = HashMap::new();
        emit_dep(
            &parts,
            "crates.awt.src.cli",
            "crates.awt.src",
            &crate_map,
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to.as_str(), "crates.awt.src.graph.coupling_graph");
        assert_eq!(out[0].from.as_str(), "crates.awt.src.cli");
    }

    #[test]
    fn test_emit_dep_super_path_should_go_up_one_level() {
        let mut out = Vec::new();
        let parts: Vec<String> = ["super", "metrics"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let crate_map = HashMap::new();
        emit_dep(
            &parts,
            "crates.awt.src.graph.coupling_graph",
            "crates.awt.src",
            &crate_map,
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to.as_str(), "crates.awt.src.graph.metrics");
    }

    #[test]
    fn test_emit_dep_std_import_should_not_emit() {
        let mut out = Vec::new();
        let parts: Vec<String> = ["std", "path", "Path"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let crate_map = HashMap::new();
        emit_dep(&parts, "src.cli", "src", &crate_map, &mut out);
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn test_emit_dep_external_crate_should_not_emit() {
        let mut out = Vec::new();
        let parts: Vec<String> = ["petgraph", "algo", "tarjan_scc"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let crate_map = HashMap::new();
        emit_dep(&parts, "src.cli", "src", &crate_map, &mut out);
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn test_extract_crate_use_should_emit_module_dep_edge() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/graph")).unwrap();
        std::fs::write(
            root.join("src/cli.rs"),
            b"use crate::graph::coupling_graph;\n",
        )
        .unwrap();
        std::fs::write(root.join("src/graph/coupling_graph.rs"), b"").unwrap();

        let deps = extract(root).unwrap();
        let actual: Vec<(&str, &str)> = deps
            .iter()
            .map(|d| (d.from.as_str(), d.to.as_str()))
            .collect();
        assert!(
            actual.contains(&("src.cli", "src.graph.coupling_graph")),
            "expected edge not found in {actual:?}"
        );
    }

    #[test]
    fn test_extract_super_use_should_resolve_to_parent_module() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/graph")).unwrap();
        std::fs::write(
            root.join("src/graph/coupling_graph.rs"),
            b"use super::metrics;\n",
        )
        .unwrap();
        std::fs::write(root.join("src/graph/metrics.rs"), b"").unwrap();

        let deps = extract(root).unwrap();
        let actual: Vec<(&str, &str)> = deps
            .iter()
            .map(|d| (d.from.as_str(), d.to.as_str()))
            .collect();
        assert!(
            actual.contains(&("src.graph.coupling_graph", "src.graph.metrics")),
            "expected edge not found in {actual:?}"
        );
    }

    #[test]
    fn test_extract_brace_use_group_should_emit_multiple_edges() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/report")).unwrap();
        std::fs::write(
            root.join("src/cli.rs"),
            b"use crate::report::{dot, terminal};\n",
        )
        .unwrap();
        std::fs::write(root.join("src/report/dot.rs"), b"").unwrap();
        std::fs::write(root.join("src/report/terminal.rs"), b"").unwrap();

        let deps = extract(root).unwrap();
        let actual: Vec<(&str, &str)> = deps
            .iter()
            .map(|d| (d.from.as_str(), d.to.as_str()))
            .collect();
        assert!(
            actual.contains(&("src.cli", "src.report.dot")),
            "{actual:?}"
        );
        assert!(
            actual.contains(&("src.cli", "src.report.terminal")),
            "{actual:?}"
        );
    }

    #[test]
    fn test_extract_external_use_should_not_emit_edge() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/cli.rs"), b"use std::path::Path;\n").unwrap();

        let deps = extract(root).unwrap();
        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_extract_alias_use_should_emit_original_module_not_alias() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/graph")).unwrap();
        std::fs::write(
            root.join("src/cli.rs"),
            b"use crate::graph::coupling_graph as cg;\n",
        )
        .unwrap();
        std::fs::write(root.join("src/graph/coupling_graph.rs"), b"").unwrap();

        let deps = extract(root).unwrap();
        let actual: Vec<(&str, &str)> = deps
            .iter()
            .map(|d| (d.from.as_str(), d.to.as_str()))
            .collect();
        assert!(
            actual.contains(&("src.cli", "src.graph.coupling_graph")),
            "{actual:?}"
        );
    }

    #[test]
    fn test_extract_mod_declaration_should_emit_dep() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), b"mod cli;\n").unwrap();
        std::fs::write(root.join("src/cli.rs"), b"").unwrap();

        let deps = extract(root).unwrap();
        let actual: Vec<(&str, &str)> = deps
            .iter()
            .map(|d| (d.from.as_str(), d.to.as_str()))
            .collect();
        assert!(
            actual.iter().any(|(from, to)| {
                from.contains("src")
                    && std::path::Path::new(to)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("cli"))
            }),
            "expected mod dependency not found in {actual:?}"
        );
    }

    #[test]
    fn test_extract_mod_inline_block_should_not_emit_dep() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            b"mod utils {\n    fn foo() {}\n}\n",
        )
        .unwrap();

        let deps = extract(root).unwrap();
        assert_eq!(
            deps.len(),
            0,
            "inline mod blocks should not emit deps: {deps:?}"
        );
    }

    #[test]
    fn test_build_crate_map_should_find_workspace_member() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("my-crate/src")).unwrap();
        std::fs::write(
            root.join("my-crate/Cargo.toml"),
            b"[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(root.join("my-crate/src/lib.rs"), b"").unwrap();

        let actual = build_crate_map(root);
        assert!(
            actual.contains_key("my_crate"),
            "expected my_crate key in map: {actual:?}"
        );
        assert_eq!(actual["my_crate"], "my-crate.src");
    }

    #[test]
    fn test_extract_external_crate_use_should_emit_dep_when_workspace_member() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("lang-core/src")).unwrap();
        std::fs::write(
            root.join("lang-core/Cargo.toml"),
            b"[package]\nname = \"lang-core\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(root.join("lang-core/src/lib.rs"), b"").unwrap();

        std::fs::create_dir_all(root.join("rs-analyzer/src")).unwrap();
        std::fs::write(
            root.join("rs-analyzer/Cargo.toml"),
            b"[package]\nname = \"rs-analyzer\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("rs-analyzer/src/lib.rs"),
            b"use lang_core::LanguageAnalyzer;\n",
        )
        .unwrap();

        let deps = extract(root).unwrap();
        let actual: Vec<(&str, &str)> = deps
            .iter()
            .map(|d| (d.from.as_str(), d.to.as_str()))
            .collect();
        assert!(
            actual.contains(&("rs-analyzer.src", "lang-core.src")),
            "expected edge from rs-analyzer to lang-core not found in {actual:?}"
        );
    }

    #[test]
    fn test_extract_third_party_crate_use_should_not_emit_dep() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("rs-analyzer/src")).unwrap();
        std::fs::write(
            root.join("rs-analyzer/Cargo.toml"),
            b"[package]\nname = \"rs-analyzer\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("rs-analyzer/src/lib.rs"),
            b"use petgraph::algo::tarjan_scc;\n",
        )
        .unwrap();

        let deps = extract(root).unwrap();
        let petgraph_deps: Vec<_> = deps.iter().filter(|d| d.to.contains("petgraph")).collect();
        assert!(
            petgraph_deps.is_empty(),
            "should not emit edge to third-party petgraph: {deps:?}"
        );
    }
}
