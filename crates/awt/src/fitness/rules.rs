use std::collections::{HashMap, HashSet};

use camino::Utf8PathBuf;

use crate::config::FitnessConfig;
use crate::graph::coupling_graph::{FileRole, GraphIndex};
use crate::graph::metrics::{Dependency, Depender, Instability, MetricsResult, violates_sdp};
use crate::model::{DistanceBand, RuleId, Severity, Violation, ViolationDetail};

use super::layer_config;

pub fn adp_no_cycles(idx: &GraphIndex, config: &FitnessConfig) -> Vec<Violation> {
    if !config.adp.enabled {
        return vec![];
    }

    let source_nodes: HashSet<_> = idx
        .graph
        .node_indices()
        .filter(|&n| idx.graph[n].role == FileRole::Source)
        .collect();

    let mut violations = vec![];

    let sccs = petgraph::algo::tarjan_scc(&idx.graph);
    for scc in sccs {
        let source_scc: Vec<_> = scc
            .into_iter()
            .filter(|n| source_nodes.contains(n))
            .collect();
        if source_scc.len() > 1 {
            let path: Vec<Utf8PathBuf> = source_scc
                .iter()
                .map(|&n| idx.graph[n].path.clone())
                .collect();
            let message = format!(
                "cycle involving {} files: {}",
                path.len(),
                path.iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(" \u{2192} ")
            );
            violations.push(Violation {
                rule: RuleId::AdpNoCycles,
                severity: Severity::Error,
                message,
                files: path.clone(),
                detail: ViolationDetail::Cycle { path },
            });
        }
    }

    for &n in &source_nodes {
        if idx.graph.find_edge(n, n).is_some() {
            let path = vec![idx.graph[n].path.clone()];
            let message = format!(
                "cycle involving {} files: {}",
                path.len(),
                path.iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(" \u{2192} ")
            );
            violations.push(Violation {
                rule: RuleId::AdpNoCycles,
                severity: Severity::Error,
                message,
                files: path.clone(),
                detail: ViolationDetail::Cycle { path },
            });
        }
    }

    violations
}

pub fn sdp_stable_dependencies(
    idx: &GraphIndex,
    metrics: &MetricsResult,
    config: &FitnessConfig,
) -> Vec<Violation> {
    if !config.sdp.enabled {
        return vec![];
    }

    let instability_map: HashMap<Utf8PathBuf, Instability> = metrics
        .nodes
        .iter()
        .map(|n| (n.file.clone(), n.instability))
        .collect();

    let mut violations = vec![];

    for edge in idx.graph.edge_indices() {
        let (src_idx, dst_idx) = idx.graph.edge_endpoints(edge).expect("edge must exist");
        let source = &idx.graph[src_idx].path;
        let target = &idx.graph[dst_idx].path;

        // Graph edge src→dst means "mutating src broke dst", so dst depends on src.
        // dependency = src (what is depended upon), depender = dst (what depends on it).
        let Some(&i_dependency) = instability_map.get(source) else {
            continue;
        };
        let Some(&i_depender) = instability_map.get(target) else {
            continue;
        };

        if violates_sdp(Dependency(i_dependency), Depender(i_depender)) {
            let i_src = i_dependency.as_f64();
            let i_dst = i_depender.as_f64();
            let delta = i_dst - i_src;
            let target_short = target.file_name().unwrap_or(target.as_str());
            let source_short = source.file_name().unwrap_or(source.as_str());
            let message = format!(
                "{source} \u{2192} {target}: I({target_short})={i_dst:.2} > I({source_short})={i_src:.2} (delta {delta:.2})"
            );
            violations.push(Violation {
                rule: RuleId::SdpStableDependencies,
                severity: Severity::Warning,
                message,
                files: vec![source.clone(), target.clone()],
                detail: ViolationDetail::UnstableDependency {
                    source: source.clone(),
                    target: target.clone(),
                    instability_source: i_src,
                    instability_target: i_dst,
                    delta,
                },
            });
        }
    }

    violations
}

pub fn main_sequence_distance(metrics: &MetricsResult, config: &FitnessConfig) -> Vec<Violation> {
    if !config.main_sequence.enabled {
        return vec![];
    }

    let mut violations = vec![];

    for node in &metrics.nodes {
        if node.role != FileRole::Source {
            continue;
        }
        let d = node.distance;

        let band = if d > config.main_sequence.error_threshold {
            DistanceBand::Error
        } else if d > config.main_sequence.warning_threshold {
            DistanceBand::Warning
        } else if d > config.main_sequence.watch_threshold {
            DistanceBand::Watch
        } else {
            continue;
        };

        let severity = match band {
            DistanceBand::Error => Severity::Error,
            DistanceBand::Warning | DistanceBand::Watch => Severity::Warning,
        };

        let band_label = match band {
            DistanceBand::Error => "error",
            DistanceBand::Warning => "warning",
            DistanceBand::Watch => "watch",
        };

        let file = node.file.clone();
        let message = format!("{file}: D={d:.2} ({band_label})");

        violations.push(Violation {
            rule: RuleId::MainSequenceDistance,
            severity,
            message,
            files: vec![file.clone()],
            detail: ViolationDetail::DistanceViolation {
                file,
                abstractness: node.abstractness,
                instability: node.instability.as_f64(),
                distance: d,
                band,
            },
        });
    }

    violations
}

pub fn dependency_rule(idx: &GraphIndex, config: &FitnessConfig) -> Vec<Violation> {
    if config.layers.is_empty() {
        return vec![];
    }

    let layer_map = layer_config::build_layer_map(idx, &config.layers);

    let layer_lookup: HashMap<&str, &crate::config::LayerConfig> =
        config.layers.iter().map(|l| (l.name.as_str(), l)).collect();

    let mut violations = vec![];

    for edge in idx.graph.edge_indices() {
        let (src_idx, dst_idx) = idx.graph.edge_endpoints(edge).expect("edge must exist");
        let source = &idx.graph[src_idx].path;
        let target = &idx.graph[dst_idx].path;

        let Some(src_layer_name) = layer_map.get_layer(src_idx) else {
            continue;
        };
        let Some(dst_layer_name) = layer_map.get_layer(dst_idx) else {
            continue;
        };

        let Some(src_layer_config) = layer_lookup.get(src_layer_name) else {
            continue;
        };

        if !src_layer_config
            .may_depend_on
            .iter()
            .any(|allowed| allowed == dst_layer_name)
        {
            let src_layer = src_layer_name.to_string();
            let dst_layer = dst_layer_name.to_string();
            let message = format!(
                "{src_layer} \u{2192} {dst_layer}: forbidden (layer '{src_layer}' may not depend on '{dst_layer}')"
            );
            violations.push(Violation {
                rule: RuleId::DependencyRule,
                severity: Severity::Error,
                message,
                files: vec![source.clone(), target.clone()],
                detail: ViolationDetail::ForbiddenDependency {
                    source: source.clone(),
                    source_layer: src_layer,
                    target: target.clone(),
                    target_layer: dst_layer,
                },
            });
        }
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AdpConfig, FitnessConfig, LayerConfig, MainSequenceConfig, SdpConfig};
    use crate::graph::coupling_graph::{CouplingEdge, CouplingGraph, CouplingNode};
    use crate::graph::metrics::{Instability, NodeMetrics};
    use std::collections::HashMap;

    fn make_graph(edges: &[(&str, &str)]) -> GraphIndex {
        make_graph_with_roles(edges, &[])
    }

    fn make_graph_with_roles(edges: &[(&str, &str)], test_files: &[&str]) -> GraphIndex {
        let test_set: HashSet<&str> = test_files.iter().copied().collect();
        let mut graph = CouplingGraph::new();
        let mut map: HashMap<&str, petgraph::graph::NodeIndex> = HashMap::new();
        for &(src, dst) in edges {
            let role_for = |p: &str| {
                if test_set.contains(p) {
                    FileRole::Test
                } else {
                    FileRole::Source
                }
            };
            let s = *map.entry(src).or_insert_with(|| {
                graph.add_node(CouplingNode {
                    path: Utf8PathBuf::from(src),
                    role: role_for(src),
                })
            });
            let d = *map.entry(dst).or_insert_with(|| {
                graph.add_node(CouplingNode {
                    path: Utf8PathBuf::from(dst),
                    role: role_for(dst),
                })
            });
            graph.add_edge(s, d, CouplingEdge { failure_count: 1 });
        }
        GraphIndex { graph }
    }

    fn make_metrics(nodes: &[(&str, f64, f64, f64)]) -> MetricsResult {
        MetricsResult {
            nodes: nodes
                .iter()
                .map(|(p, i, a, d)| NodeMetrics {
                    file: Utf8PathBuf::from(*p),
                    role: FileRole::Source,
                    fan_in: 0,
                    fan_out: 0,
                    instability: Instability::new(*i),
                    abstractness: *a,
                    distance: *d,
                    distance_warning: *d > 0.3,
                    distance_failure: *d > 0.5,
                })
                .collect(),
        }
    }

    fn adp_enabled() -> FitnessConfig {
        FitnessConfig {
            adp: AdpConfig { enabled: true },
            sdp: SdpConfig { enabled: false },
            main_sequence: MainSequenceConfig {
                enabled: false,
                ..MainSequenceConfig::default()
            },
            layers: vec![],
        }
    }

    fn sdp_enabled() -> FitnessConfig {
        FitnessConfig {
            adp: AdpConfig { enabled: false },
            sdp: SdpConfig { enabled: true },
            main_sequence: MainSequenceConfig {
                enabled: false,
                ..MainSequenceConfig::default()
            },
            layers: vec![],
        }
    }

    fn ms_enabled() -> FitnessConfig {
        FitnessConfig {
            adp: AdpConfig { enabled: false },
            sdp: SdpConfig { enabled: false },
            main_sequence: MainSequenceConfig {
                enabled: true,
                ..MainSequenceConfig::default()
            },
            layers: vec![],
        }
    }

    fn layers_config(layers: Vec<LayerConfig>) -> FitnessConfig {
        FitnessConfig {
            adp: AdpConfig { enabled: false },
            sdp: SdpConfig { enabled: false },
            main_sequence: MainSequenceConfig {
                enabled: false,
                ..MainSequenceConfig::default()
            },
            layers,
        }
    }

    // ── adp_no_cycles ────────────────────────────────────────────────────────

    #[test]
    fn test_adp_mutual_edge_should_produce_one_cycle_violation() {
        let idx = make_graph(&[("src/a.py", "src/b.py"), ("src/b.py", "src/a.py")]);
        let config = adp_enabled();
        let actual = adp_no_cycles(&idx, &config);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].severity, Severity::Error);
    }

    #[test]
    fn test_adp_three_node_cycle_should_produce_one_violation() {
        let idx = make_graph(&[
            ("src/a.py", "src/b.py"),
            ("src/b.py", "src/c.py"),
            ("src/c.py", "src/a.py"),
        ]);
        let config = adp_enabled();
        let actual = adp_no_cycles(&idx, &config);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].files.len(), 3);
    }

    #[test]
    fn test_adp_self_loop_should_produce_violation() {
        let mut graph = CouplingGraph::new();
        let n = graph.add_node(CouplingNode {
            path: Utf8PathBuf::from("src/a.py"),
            role: FileRole::Source,
        });
        graph.add_edge(n, n, CouplingEdge { failure_count: 1 });
        let idx = GraphIndex { graph };
        let config = adp_enabled();
        let actual = adp_no_cycles(&idx, &config);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].severity, Severity::Error);
    }

    #[test]
    fn test_adp_dag_should_produce_no_violations() {
        let idx = make_graph(&[("src/a.py", "src/b.py"), ("src/b.py", "src/c.py")]);
        let config = adp_enabled();
        let actual = adp_no_cycles(&idx, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_adp_disabled_should_produce_no_violations() {
        let idx = make_graph(&[("src/a.py", "src/b.py"), ("src/b.py", "src/a.py")]);
        let config = FitnessConfig {
            adp: AdpConfig { enabled: false },
            ..adp_enabled()
        };
        let actual = adp_no_cycles(&idx, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_adp_test_only_cycle_should_produce_no_violations() {
        let idx = make_graph_with_roles(
            &[
                ("tests/test_a.py", "tests/test_b.py"),
                ("tests/test_b.py", "tests/test_a.py"),
            ],
            &["tests/test_a.py", "tests/test_b.py"],
        );
        let config = adp_enabled();
        let actual = adp_no_cycles(&idx, &config);
        assert_eq!(actual.len(), 0);
    }

    // ── sdp_stable_dependencies ──────────────────────────────────────────────

    #[test]
    fn test_sdp_unstable_dependency_should_produce_violation() {
        let idx = make_graph(&[("src/a.py", "src/b.py")]);
        let metrics = make_metrics(&[("src/a.py", 0.2, 0.0, 0.8), ("src/b.py", 0.8, 0.0, 0.2)]);
        let config = sdp_enabled();
        let actual = sdp_stable_dependencies(&idx, &metrics, &config);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].severity, Severity::Warning);
    }

    #[test]
    fn test_sdp_stable_dependency_should_produce_no_violation() {
        let idx = make_graph(&[("src/a.py", "src/b.py")]);
        let metrics = make_metrics(&[("src/a.py", 0.8, 0.0, 0.2), ("src/b.py", 0.2, 0.0, 0.8)]);
        let config = sdp_enabled();
        let actual = sdp_stable_dependencies(&idx, &metrics, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_sdp_equal_instability_within_epsilon_should_produce_no_violation() {
        let idx = make_graph(&[("src/a.py", "src/b.py")]);
        let metrics = make_metrics(&[("src/a.py", 0.5, 0.0, 0.5), ("src/b.py", 0.5, 0.0, 0.5)]);
        let config = sdp_enabled();
        let actual = sdp_stable_dependencies(&idx, &metrics, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_sdp_isolated_nodes_should_produce_no_violation() {
        let idx = make_graph(&[("src/a.py", "src/b.py")]);
        let metrics = make_metrics(&[("src/a.py", 1.0, 0.0, 0.0), ("src/b.py", 1.0, 0.0, 0.0)]);
        let config = sdp_enabled();
        let actual = sdp_stable_dependencies(&idx, &metrics, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_sdp_disabled_should_produce_no_violations() {
        let idx = make_graph(&[("src/a.py", "src/b.py")]);
        let metrics = make_metrics(&[("src/a.py", 0.2, 0.0, 0.8), ("src/b.py", 0.8, 0.0, 0.2)]);
        let config = FitnessConfig {
            sdp: SdpConfig { enabled: false },
            ..sdp_enabled()
        };
        let actual = sdp_stable_dependencies(&idx, &metrics, &config);
        assert_eq!(actual.len(), 0);
    }

    // ── main_sequence_distance ───────────────────────────────────────────────

    #[test]
    fn test_main_sequence_high_distance_should_produce_error() {
        let metrics = make_metrics(&[("src/a.py", 0.5, 0.5, 0.6)]);
        let config = ms_enabled();
        let actual = main_sequence_distance(&metrics, &config);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].severity, Severity::Error);
    }

    #[test]
    fn test_main_sequence_warning_distance_should_produce_warning() {
        let metrics = make_metrics(&[("src/a.py", 0.5, 0.5, 0.4)]);
        let config = ms_enabled();
        let actual = main_sequence_distance(&metrics, &config);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].severity, Severity::Warning);
    }

    #[test]
    fn test_main_sequence_watch_distance_should_produce_warning() {
        let metrics = make_metrics(&[("src/a.py", 0.5, 0.5, 0.25)]);
        let config = ms_enabled();
        let actual = main_sequence_distance(&metrics, &config);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].severity, Severity::Warning);
    }

    #[test]
    fn test_main_sequence_healthy_distance_should_produce_no_violation() {
        let metrics = make_metrics(&[("src/a.py", 0.5, 0.5, 0.1)]);
        let config = ms_enabled();
        let actual = main_sequence_distance(&metrics, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_main_sequence_test_node_should_be_skipped() {
        let mut metrics = make_metrics(&[("tests/test_a.py", 0.5, 0.5, 0.6)]);
        metrics.nodes[0].role = FileRole::Test;
        let config = ms_enabled();
        let actual = main_sequence_distance(&metrics, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_main_sequence_on_sequence_should_produce_no_violation() {
        let metrics = make_metrics(&[("src/a.py", 0.5, 0.5, 0.0)]);
        let config = ms_enabled();
        let actual = main_sequence_distance(&metrics, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_main_sequence_disabled_should_produce_no_violations() {
        let metrics = make_metrics(&[("src/a.py", 0.5, 0.5, 0.6)]);
        let config = FitnessConfig {
            main_sequence: MainSequenceConfig {
                enabled: false,
                ..MainSequenceConfig::default()
            },
            ..ms_enabled()
        };
        let actual = main_sequence_distance(&metrics, &config);
        assert_eq!(actual.len(), 0);
    }

    // ── dependency_rule ──────────────────────────────────────────────────────

    fn stub_layers() -> Vec<LayerConfig> {
        vec![
            LayerConfig {
                name: "domain".to_string(),
                paths: vec!["src/domain/**".to_string()],
                may_depend_on: vec![],
            },
            LayerConfig {
                name: "usecases".to_string(),
                paths: vec!["src/usecases/**".to_string()],
                may_depend_on: vec!["domain".to_string()],
            },
        ]
    }

    #[test]
    fn test_dependency_rule_forbidden_dep_should_produce_violation() {
        let idx = make_graph(&[("src/domain/order.py", "src/usecases/order_service.py")]);
        let config = layers_config(stub_layers());
        let actual = dependency_rule(&idx, &config);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].severity, Severity::Error);
    }

    #[test]
    fn test_dependency_rule_allowed_dep_should_produce_no_violation() {
        let idx = make_graph(&[("src/usecases/order_service.py", "src/domain/order.py")]);
        let config = layers_config(stub_layers());
        let actual = dependency_rule(&idx, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_dependency_rule_file_not_in_layer_should_be_skipped() {
        let idx = make_graph(&[("src/infra/db.py", "src/domain/order.py")]);
        let config = layers_config(stub_layers());
        let actual = dependency_rule(&idx, &config);
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_dependency_rule_empty_layers_should_produce_no_violations() {
        let idx = make_graph(&[("src/domain/order.py", "src/usecases/order_service.py")]);
        let config = layers_config(vec![]);
        let actual = dependency_rule(&idx, &config);
        assert_eq!(actual.len(), 0);
    }
}
