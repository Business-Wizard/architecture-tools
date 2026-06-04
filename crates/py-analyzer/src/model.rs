use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResult {
    pub module_deps: Vec<ModuleDep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDep {
    pub from: String,
    pub to: String,
}
