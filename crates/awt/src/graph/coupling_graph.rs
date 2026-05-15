use std::collections::HashMap;

use camino::Utf8PathBuf;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::model::MutantResult;

#[derive(Debug, Clone)]
pub struct CouplingNode {
    pub path: Utf8PathBuf,
    pub is_test_code: bool,
}

#[derive(Debug, Clone)]
pub struct CouplingEdge {
    pub failure_count: usize,
}

pub type CouplingGraph = DiGraph<CouplingNode, CouplingEdge>;

pub struct GraphIndex {
    pub graph: CouplingGraph,
    pub node_map: HashMap<Utf8PathBuf, NodeIndex>,
}

impl GraphIndex {
    pub fn build(results: &[MutantResult]) -> Self {
        let mut graph = CouplingGraph::new();
        let mut node_map: HashMap<Utf8PathBuf, NodeIndex> = HashMap::new();

        let get_or_insert = |g: &mut CouplingGraph,
                             map: &mut HashMap<Utf8PathBuf, NodeIndex>,
                             path: Utf8PathBuf| {
            if let Some(&idx) = map.get(&path) {
                return idx;
            }
            let is_test_code = is_test_file(&path);
            let idx = g.add_node(CouplingNode {
                path: path.clone(),
                is_test_code,
            });
            map.insert(path, idx);
            idx
        };

        for result in results {
            if result.external_failures.is_empty() {
                continue;
            }

            let src_idx = get_or_insert(&mut graph, &mut node_map, result.candidate.file.clone());

            let mut affected: HashMap<Utf8PathBuf, usize> = HashMap::new();
            for f in &result.external_failures {
                *affected.entry(f.file.clone()).or_insert(0) += 1;
            }

            for (affected_file, count) in affected {
                let dst_idx = get_or_insert(&mut graph, &mut node_map, affected_file);
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

        GraphIndex { graph, node_map }
    }
}

fn is_test_file(path: &Utf8PathBuf) -> bool {
    let s = path.as_str();
    s.contains("/tests/")
        || s.contains("/test_")
        || s.ends_with("_test.py")
        || s.starts_with("tests/")
        || s.starts_with("test_")
}
