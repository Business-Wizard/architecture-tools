use serde::{Deserialize, Serialize};

pub use lang_core::ModuleDep;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResult {
    pub module_deps: Vec<ModuleDep>,
    pub classes: Vec<ClassDef>,
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
