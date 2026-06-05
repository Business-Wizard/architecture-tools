use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MainSequenceConfig {
    pub enabled: bool,
    pub watch_threshold: f64,
    pub warning_threshold: f64,
    pub error_threshold: f64,
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
