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
    pub exclude_dirs: Vec<String>,
    pub operators: OperatorConfig,
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
            jobs: num_cpus(),
            timeout_secs: 60,
            keep_temp_on_failure: false,
            exclude_dirs: vec![
                ".git".into(),
                ".venv".into(),
                "__pycache__".into(),
                ".mypy_cache".into(),
                ".pytest_cache".into(),
                ".ruff_cache".into(),
                "node_modules".into(),
            ],
            operators: OperatorConfig::default(),
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

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(4)
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
