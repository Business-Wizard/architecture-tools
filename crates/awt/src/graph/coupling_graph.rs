use std::collections::HashMap;

use camino::Utf8PathBuf;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::model::MutantResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRole {
    Source,
    Test,
}

impl FileRole {
    pub fn from_path(path: &Utf8PathBuf) -> Self {
        let s = path.as_str();
        if s.contains("/tests/")
            || s.contains("/test_")
            || s.ends_with("_test.py")
            || s.starts_with("tests/")
            || s.starts_with("test_")
        {
            FileRole::Test
        } else {
            FileRole::Source
        }
    }
}

#[derive(Debug, Clone)]
pub struct CouplingNode {
    pub path: Utf8PathBuf,
    pub role: FileRole,
}

#[derive(Debug, Clone)]
pub struct CouplingEdge {
    pub failure_count: usize,
}

pub type CouplingGraph = DiGraph<CouplingNode, CouplingEdge>;

pub struct GraphIndex {
    pub graph: CouplingGraph,
}

pub(crate) fn resolve_source_module(
    test_path: &Utf8PathBuf,
    known_source_files: &[Utf8PathBuf],
) -> Option<Utf8PathBuf> {
    if FileRole::from_path(test_path) == FileRole::Source {
        return None;
    }
    let stem = test_path.file_stem()?;
    let candidate_stem = if let Some(s) = stem.strip_prefix("test_") {
        s.to_string()
    } else if let Some(s) = stem.strip_suffix("_test") {
        s.to_string()
    } else {
        return None;
    };
    let target_filename = format!("{candidate_stem}.py");
    let matches: Vec<&Utf8PathBuf> = known_source_files
        .iter()
        .filter(|f| f.file_name() == Some(target_filename.as_str()))
        .collect();
    match matches.len() {
        0 => None,
        1 => Some(matches[0].clone()),
        _ => {
            let test_parent = test_path.parent().map_or("", camino::Utf8Path::as_str);
            matches
                .into_iter()
                .max_by(|a, b| {
                    let common_a = common_prefix_len(
                        test_parent,
                        a.parent().map_or("", camino::Utf8Path::as_str),
                    );
                    let common_b = common_prefix_len(
                        test_parent,
                        b.parent().map_or("", camino::Utf8Path::as_str),
                    );
                    common_a.cmp(&common_b).then(a.as_str().cmp(b.as_str()))
                })
                .cloned()
        }
    }
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

fn get_or_insert_node(
    g: &mut CouplingGraph,
    map: &mut HashMap<Utf8PathBuf, NodeIndex>,
    path: Utf8PathBuf,
) -> NodeIndex {
    if let Some(&idx) = map.get(&path) {
        return idx;
    }
    let role = FileRole::from_path(&path);
    let idx = g.add_node(CouplingNode {
        path: path.clone(),
        role,
    });
    map.insert(path, idx);
    idx
}

fn build_module_map(source_files: &[Utf8PathBuf]) -> HashMap<String, Utf8PathBuf> {
    let mut map = HashMap::new();
    for file in source_files {
        let s = file.as_str();
        let without_ext = s.strip_suffix(".py").unwrap_or(s);
        let is_init = without_ext.ends_with("/__init__") || without_ext == "__init__";
        let dotted = if is_init {
            without_ext
                .strip_suffix("/__init__")
                .unwrap_or(without_ext)
                .replace('/', ".")
        } else {
            without_ext.replace('/', ".")
        };
        let parts: Vec<&str> = dotted.split('.').collect();
        for start in 0..parts.len() {
            let suffix = parts[start..].join(".");
            map.entry(suffix).or_insert_with(|| file.clone());
        }
    }
    map
}

impl GraphIndex {
    pub fn build(results: &[MutantResult], known_source_files: &[Utf8PathBuf]) -> Self {
        let mut graph = CouplingGraph::new();
        let mut node_map: HashMap<Utf8PathBuf, NodeIndex> = HashMap::new();

        for result in results {
            if result.external_failures.is_empty() {
                continue;
            }

            let src_idx =
                get_or_insert_node(&mut graph, &mut node_map, result.candidate.file.clone());

            let mut affected: HashMap<Utf8PathBuf, usize> = HashMap::new();
            for f in &result.external_failures {
                *affected.entry(f.file.clone()).or_insert(0) += 1;
            }

            for (affected_file, count) in affected {
                let resolved = if FileRole::from_path(&affected_file) == FileRole::Test {
                    match resolve_source_module(&affected_file, known_source_files) {
                        Some(src) => src,
                        None => continue,
                    }
                } else {
                    affected_file
                };

                if resolved == result.candidate.file {
                    continue;
                }

                let dst_idx = get_or_insert_node(&mut graph, &mut node_map, resolved);
                if let Some(e) = graph.find_edge(src_idx, dst_idx) {
                    graph[e].failure_count += count;
                } else {
                    graph.add_edge(
                        src_idx,
                        dst_idx,
                        CouplingEdge {
                            failure_count: count,
                        },
                    );
                }
            }
        }

        GraphIndex { graph }
    }

    pub fn build_from_source_imports(
        source_files: &[Utf8PathBuf],
        repo_root: &std::path::Path,
    ) -> Self {
        let module_map = build_module_map(source_files);
        let mut graph = CouplingGraph::new();
        let mut node_map: HashMap<Utf8PathBuf, NodeIndex> = HashMap::new();

        for file in source_files {
            let abs = repo_root.join(file.as_str());
            let Ok(source) = std::fs::read(&abs) else {
                continue;
            };
            let Some(parsed) = crate::python_ast::ParsedFile::parse(&source) else {
                continue;
            };

            for imp in crate::python_ast::find_imports(&parsed) {
                for module_name in crate::python_ast::extract_module_names(&imp.module_path) {
                    let Some(target) = module_map.get(&module_name) else {
                        continue;
                    };
                    if target == file {
                        continue;
                    }
                    // Edge: target (dependency) → file (importer/depender)
                    // Matches build() semantics: src=dependency, dst=depender
                    let target_idx = get_or_insert_node(&mut graph, &mut node_map, target.clone());
                    let file_idx = get_or_insert_node(&mut graph, &mut node_map, file.clone());
                    if let Some(e) = graph.find_edge(target_idx, file_idx) {
                        graph[e].failure_count += 1;
                    } else {
                        graph.add_edge(target_idx, file_idx, CouplingEdge { failure_count: 1 });
                    }
                }
            }
        }

        GraphIndex { graph }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CandidateKind, FailureCategory, FailureEvent, FailureScope, MutantId, MutantStatus,
        OperatorKind, VerifierKind,
    };

    fn make_result(candidate_file: &str, external_files: &[&str]) -> MutantResult {
        let candidate = crate::model::Candidate {
            id: MutantId(format!("{candidate_file}::func::op")),
            file: Utf8PathBuf::from(candidate_file),
            symbol: "func".into(),
            kind: CandidateKind::Function,
            operator: OperatorKind::AddRequiredParameter,
            line: 1,
            byte_start: 0,
            byte_end: 0,
        };
        let external_failures = external_files
            .iter()
            .map(|f| FailureEvent {
                mutant_id: candidate.id.clone(),
                command: VerifierKind::Pytest,
                file: Utf8PathBuf::from(*f),
                line: None,
                column: None,
                symbol: None,
                category: FailureCategory::TestAssertion,
                message: String::new(),
                scope: FailureScope::External,
            })
            .collect();
        MutantResult {
            candidate,
            status: MutantStatus::Breaks,
            local_failures: vec![],
            external_failures,
        }
    }

    #[test]
    fn test_is_test_file_with_tests_directory_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/tests/helper.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_test_prefix_in_path_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/test_utils.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_underscore_test_suffix_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/order_test.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_top_level_tests_prefix_should_return_true() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("tests/test_order.py"));
        assert_eq!(actual, FileRole::Test);
    }

    #[test]
    fn test_is_test_file_with_source_path_should_return_false() {
        let actual = FileRole::from_path(&Utf8PathBuf::from("src/domain/order.py"));
        assert_eq!(actual, FileRole::Source);
    }

    #[test]
    fn test_build_should_skip_result_with_no_external_failures() {
        let result = make_result("src/a.py", &[]);
        let idx = GraphIndex::build(&[result], &[]);
        assert_eq!(idx.graph.node_count(), 0);
    }

    #[test]
    fn test_build_should_add_edge_between_mutated_and_affected_file() {
        let result = make_result("src/a.py", &["src/b.py"]);
        let idx = GraphIndex::build(&[result], &[]);
        assert_eq!(idx.graph.edge_count(), 1);
    }

    #[test]
    fn test_build_should_accumulate_edge_weight_for_repeated_failures() {
        let r1 = make_result("src/a.py", &["src/b.py"]);
        let r2 = make_result("src/a.py", &["src/b.py"]);
        let idx = GraphIndex::build(&[r1, r2], &[]);
        let edge = idx.graph.edge_indices().next().unwrap();
        assert_eq!(idx.graph[edge].failure_count, 2);
    }

    #[test]
    fn test_resolve_source_module_with_test_prefix_should_find_matching_source() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/test_order.py"),
            &[Utf8PathBuf::from("src/order.py")],
        );
        assert_eq!(actual, Some(Utf8PathBuf::from("src/order.py")));
    }

    #[test]
    fn test_resolve_source_module_with_test_suffix_should_find_matching_source() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/order_test.py"),
            &[Utf8PathBuf::from("src/order.py")],
        );
        assert_eq!(actual, Some(Utf8PathBuf::from("src/order.py")));
    }

    #[test]
    fn test_resolve_source_module_with_no_match_should_return_none() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/test_order.py"),
            &[Utf8PathBuf::from("src/billing.py")],
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_resolve_source_module_with_source_file_should_return_none() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("src/order.py"),
            &[Utf8PathBuf::from("src/order.py")],
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_resolve_source_module_with_unrecognised_naming_should_return_none() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/order_spec.py"),
            &[Utf8PathBuf::from("src/order.py")],
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_resolve_source_module_with_multiple_candidates_prefers_closest_directory() {
        let actual = resolve_source_module(
            &Utf8PathBuf::from("src/orders/tests/test_order.py"),
            &[
                Utf8PathBuf::from("src/orders/order.py"),
                Utf8PathBuf::from("lib/order.py"),
            ],
        );
        assert_eq!(actual, Some(Utf8PathBuf::from("src/orders/order.py")));
    }

    #[test]
    fn test_build_with_test_failure_for_known_source_should_resolve_edge_to_source() {
        let result = make_result("src/billing.py", &["tests/test_order.py"]);
        let known = vec![
            Utf8PathBuf::from("src/billing.py"),
            Utf8PathBuf::from("src/order.py"),
        ];
        let idx = GraphIndex::build(&[result], &known);
        assert_eq!(idx.graph.edge_count(), 1);
        let edge = idx.graph.edge_indices().next().unwrap();
        let (_, dst) = idx.graph.edge_endpoints(edge).unwrap();
        assert_eq!(idx.graph[dst].path, Utf8PathBuf::from("src/order.py"));
    }

    #[test]
    fn test_build_with_own_test_failure_should_drop_self_edge() {
        let result = make_result("src/order.py", &["tests/test_order.py"]);
        let known = vec![Utf8PathBuf::from("src/order.py")];
        let idx = GraphIndex::build(&[result], &known);
        assert_eq!(idx.graph.edge_count(), 0);
    }

    #[test]
    fn test_build_with_test_failure_and_no_known_source_should_drop_edge() {
        let result = make_result("src/billing.py", &["tests/test_helper.py"]);
        let idx = GraphIndex::build(&[result], &[]);
        assert_eq!(idx.graph.edge_count(), 0);
    }

    #[test]
    fn test_build_with_mixed_source_and_test_failures_should_deduplicate_resolved_edge() {
        let result = make_result("src/billing.py", &["src/order.py", "tests/test_order.py"]);
        let known = vec![
            Utf8PathBuf::from("src/billing.py"),
            Utf8PathBuf::from("src/order.py"),
        ];
        let idx = GraphIndex::build(&[result], &known);
        assert_eq!(idx.graph.edge_count(), 1);
    }

    #[test]
    fn test_build_with_multiple_test_failures_should_produce_one_edge_per_resolved_source() {
        let result = make_result(
            "src/billing.py",
            &[
                "tests/test_order.py",
                "tests/test_payment.py",
                "tests/test_invoice.py",
            ],
        );
        let known = vec![
            Utf8PathBuf::from("src/billing.py"),
            Utf8PathBuf::from("src/order.py"),
            Utf8PathBuf::from("src/payment.py"),
            Utf8PathBuf::from("src/invoice.py"),
        ];
        let idx = GraphIndex::build(&[result], &known);
        assert_eq!(idx.graph.edge_count(), 3);
    }

    #[test]
    fn test_build_with_multiple_test_failures_should_accumulate_weight_when_resolved_to_same_target()
     {
        // Two different test files that both resolve to src/order.py
        // (would happen if a repo has both tests/test_order.py and tests/order_test.py)
        let result = make_result(
            "src/billing.py",
            &["tests/test_order.py", "tests/order_test.py"],
        );
        let known = vec![
            Utf8PathBuf::from("src/billing.py"),
            Utf8PathBuf::from("src/order.py"),
        ];
        let idx = GraphIndex::build(&[result], &known);
        assert_eq!(idx.graph.edge_count(), 1);
        let edge = idx.graph.edge_indices().next().unwrap();
        assert_eq!(idx.graph[edge].failure_count, 2);
    }

    #[test]
    fn test_resolve_source_module_with_equal_prefix_length_should_tie_break_alphabetically() {
        // both candidates have the same common prefix with the test parent
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/test_order.py"),
            &[
                Utf8PathBuf::from("alpha/order.py"),
                Utf8PathBuf::from("zeta/order.py"),
            ],
        );
        assert_eq!(actual, Some(Utf8PathBuf::from("zeta/order.py")));
    }

    #[test]
    fn test_resolve_source_module_with_no_stem_should_return_none() {
        // Utf8PathBuf::from(".py") has no file stem — exercise the `?` early-exit
        let actual = resolve_source_module(
            &Utf8PathBuf::from("tests/.py"),
            &[Utf8PathBuf::from("src/order.py")],
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_build_module_map_should_include_all_dotted_suffixes() {
        let files = vec![Utf8PathBuf::from("src/domain/order.py")];
        let map = build_module_map(&files);
        assert_eq!(
            map.get("src.domain.order"),
            Some(&Utf8PathBuf::from("src/domain/order.py"))
        );
        assert_eq!(
            map.get("domain.order"),
            Some(&Utf8PathBuf::from("src/domain/order.py"))
        );
        assert_eq!(
            map.get("order"),
            Some(&Utf8PathBuf::from("src/domain/order.py"))
        );
    }

    #[test]
    fn test_build_module_map_init_file_should_map_to_package_name() {
        let files = vec![Utf8PathBuf::from("src/domain/__init__.py")];
        let map = build_module_map(&files);
        assert_eq!(
            map.get("src.domain"),
            Some(&Utf8PathBuf::from("src/domain/__init__.py"))
        );
        assert_eq!(
            map.get("domain"),
            Some(&Utf8PathBuf::from("src/domain/__init__.py"))
        );
        assert!(!map.contains_key("__init__"));
    }

    #[test]
    fn test_build_from_source_imports_should_add_edge_from_dependency_to_importer() {
        // order.py imports billing → edge: billing → order (billing is the dependency)
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("order.py"), b"import billing\n").unwrap();
        std::fs::write(root.join("billing.py"), b"").unwrap();

        let files = vec![
            Utf8PathBuf::from("order.py"),
            Utf8PathBuf::from("billing.py"),
        ];
        let idx = GraphIndex::build_from_source_imports(&files, root);

        assert_eq!(idx.graph.edge_count(), 1);
        let edge = idx.graph.edge_indices().next().unwrap();
        let (src, dst) = idx.graph.edge_endpoints(edge).unwrap();
        assert_eq!(idx.graph[src].path, Utf8PathBuf::from("billing.py"));
        assert_eq!(idx.graph[dst].path, Utf8PathBuf::from("order.py"));
    }

    #[test]
    fn test_build_from_source_imports_should_not_add_self_edges() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("order.py"), b"import order\n").unwrap();

        let files = vec![Utf8PathBuf::from("order.py")];
        let idx = GraphIndex::build_from_source_imports(&files, root);

        assert_eq!(idx.graph.edge_count(), 0);
    }

    #[test]
    fn test_build_from_source_imports_should_skip_third_party_imports() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("order.py"), b"import requests\nimport os\n").unwrap();

        let files = vec![Utf8PathBuf::from("order.py")];
        let idx = GraphIndex::build_from_source_imports(&files, root);

        assert_eq!(idx.graph.edge_count(), 0);
    }
}
