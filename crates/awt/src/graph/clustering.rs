use std::collections::HashMap;

use camino::Utf8PathBuf;
use petgraph::algo::connected_components;
use petgraph::visit::EdgeRef;

use crate::graph::coupling_graph::GraphIndex;

#[derive(Debug)]
pub struct CenterOfGravity {
    pub file: Utf8PathBuf,
    pub affected_source_code: usize,
    pub affected_test_code: usize,
    pub top_package: String,
}

#[derive(Debug)]
pub struct UnexpectedCoupling {
    pub mutant_file: Utf8PathBuf,
    pub affected_file: Utf8PathBuf,
    pub failure_count: usize,
}

#[derive(Debug)]
pub struct ClusteringResult {
    pub centers: Vec<CenterOfGravity>,
    pub unexpected: Vec<UnexpectedCoupling>,
    pub component_count: usize,
}

pub fn analyse(idx: &GraphIndex) -> ClusteringResult {
    let component_count = connected_components(&idx.graph);

    // Centers of gravity: nodes with highest out-degree (by distinct affected files)
    let mut centers: Vec<CenterOfGravity> = idx
        .graph
        .node_indices()
        .filter_map(|n| {
            let node = &idx.graph[n];
            let edges: Vec<_> = idx.graph.edges(n).collect();
            if edges.is_empty() {
                return None;
            }
            let mut source_code = 0usize;
            let mut test_code = 0usize;
            for e in &edges {
                let target = &idx.graph[e.target()];
                if target.is_test_code {
                    test_code += 1;
                } else {
                    source_code += 1;
                }
            }
            let top_package = top_package(&node.path);
            Some(CenterOfGravity {
                file: node.path.clone(),
                affected_source_code: source_code,
                affected_test_code: test_code,
                top_package,
            })
        })
        .collect();

    centers.sort_by(|a, b| {
        (b.affected_source_code + b.affected_test_code)
            .cmp(&(a.affected_source_code + a.affected_test_code))
    });

    // Unexpected coupling: edge where source and target have different top-level packages
    let mut unexpected: Vec<UnexpectedCoupling> = idx
        .graph
        .edge_indices()
        .filter_map(|e| {
            let (src, dst) = idx.graph.edge_endpoints(e)?;
            let src_pkg = top_package(&idx.graph[src].path);
            let dst_pkg = top_package(&idx.graph[dst].path);
            if src_pkg == dst_pkg {
                return None;
            }
            Some(UnexpectedCoupling {
                mutant_file: idx.graph[src].path.clone(),
                affected_file: idx.graph[dst].path.clone(),
                failure_count: idx.graph[e].failure_count,
            })
        })
        .collect();

    unexpected.sort_by(|a, b| b.failure_count.cmp(&a.failure_count));

    ClusteringResult {
        centers,
        unexpected,
        component_count,
    }
}

fn top_package(path: &Utf8PathBuf) -> String {
    path.components()
        .next()
        .map(|c| c.as_str().to_string())
        .unwrap_or_default()
}

pub fn refactor_hints(centers: &[CenterOfGravity]) -> Vec<String> {
    centers
        .iter()
        .take(5)
        .map(|c| {
            let total = c.affected_source_code + c.affected_test_code;
            format!(
                "{}: {} callers across {} source + {} test files — consider extracting an interface",
                c.file, total, c.affected_source_code, c.affected_test_code
            )
        })
        .collect()
}

/// Package distance: number of differing path components between two files.
pub fn package_distance(a: &Utf8PathBuf, b: &Utf8PathBuf) -> usize {
    let a_parts: Vec<&str> = a.components().map(|c| c.as_str()).collect();
    let b_parts: Vec<&str> = b.components().map(|c| c.as_str()).collect();
    let common = a_parts
        .iter()
        .zip(b_parts.iter())
        .take_while(|(x, y)| x == y)
        .count();
    (a_parts.len() - common) + (b_parts.len() - common)
}

#[allow(dead_code)]
fn count_by_package(centers: &[CenterOfGravity]) -> HashMap<String, usize> {
    let mut map: HashMap<String, usize> = HashMap::new();
    for c in centers {
        *map.entry(c.top_package.clone()).or_insert(0) += 1;
    }
    map
}
