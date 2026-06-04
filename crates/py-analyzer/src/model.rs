use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResult {
    pub module_deps: Vec<ModuleDep>,
    pub classes: Vec<ClassDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDep {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassDef {
    pub module: String,
    pub name: String,
    pub bases: Vec<String>,
    pub attributes: Vec<String>,
    pub methods: Vec<String>,
    pub class_deps: Vec<String>,
}
