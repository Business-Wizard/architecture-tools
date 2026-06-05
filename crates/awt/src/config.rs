use std::path::Path;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::model::ConfigError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub max_mutants: usize,
    pub jobs: usize,
    pub timeout_secs: u64,
    pub keep_temp_on_failure: bool,
    pub include_dirs: Vec<String>,
    pub operators: OperatorConfig,
    pub fitness: FitnessConfig,
    pub graph_analysis: graph_analysis::GraphLayerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct OperatorConfig {
    pub add_required_parameter: bool,
    pub rename_parameter: bool,
    pub remove_parameter: bool,
    pub remove_import: bool,
    pub remove_module: bool,
    pub move_module: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_mutants: 500,
            jobs: default_jobs(),
            timeout_secs: 60,
            keep_temp_on_failure: false,
            include_dirs: vec!["src".into()],
            operators: OperatorConfig::default(),
            fitness: FitnessConfig::default(),
            graph_analysis: graph_analysis::GraphLayerConfig::default(),
        }
    }
}

impl Default for OperatorConfig {
    fn default() -> Self {
        Self {
            add_required_parameter: true,
            rename_parameter: false,
            remove_parameter: false,
            remove_import: false,
            remove_module: false,
            move_module: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FitnessConfig {
    pub adp: AdpConfig,
    pub sdp: SdpConfig,
    pub main_sequence: MainSequenceConfig,
    pub layers: Vec<LayerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdpConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SdpConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MainSequenceConfig {
    pub enabled: bool,
    pub watch_threshold: f64,
    pub warning_threshold: f64,
    pub error_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    pub name: String,
    pub paths: Vec<String>,
    pub may_depend_on: Vec<String>,
}

impl Default for AdpConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for SdpConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for MainSequenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            watch_threshold: 0.2,
            warning_threshold: 0.3,
            error_threshold: 0.5,
        }
    }
}

fn default_jobs() -> usize {
    let cpus = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    cpus.saturating_sub(1).max(1)
}

pub fn load(config_path: Option<&Utf8PathBuf>, repo_root: &Path) -> Result<Config, ConfigError> {
    let candidate = config_path.map_or_else(
        || repo_root.join("awt.toml"),
        |p| p.as_std_path().to_path_buf(),
    );

    if !candidate.exists() {
        return Ok(Config::default());
    }

    let raw = std::fs::read_to_string(&candidate)?;
    let config: Config = toml::from_str(&raw)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fitness_config_default_should_have_all_rules_enabled() {
        let cfg = FitnessConfig::default();
        assert!(cfg.adp.enabled && cfg.sdp.enabled && cfg.main_sequence.enabled);
    }

    #[test]
    fn test_graph_analysis_config_from_toml_should_parse_layers() {
        let toml = r#"
[[graph_analysis.layers]]
name = "domain"
module_prefixes = ["domain"]

[[graph_analysis.layers]]
name = "infra"
module_prefixes = ["feature.postgres_repo", "feature.local_file_repo"]
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.graph_analysis.layers.len(), 2);
        assert_eq!(cfg.graph_analysis.layers[0].name, "domain");
        assert_eq!(cfg.graph_analysis.layers[1].module_prefixes.len(), 2);
    }

    #[test]
    fn test_fitness_config_from_toml_should_parse_layers() {
        let toml = r#"
[[fitness.layers]]
name = "domain"
paths = ["src/domain/**"]
may_depend_on = []
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.fitness.layers.len(), 1);
        assert_eq!(cfg.fitness.layers[0].name, "domain");
    }
}
