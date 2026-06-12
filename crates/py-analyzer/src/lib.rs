mod error;
mod python_imports;

pub use error::InspectorError;

use std::path::Path;

pub struct PythonAnalyzer;

impl lang_core::LanguageAnalyzer for PythonAnalyzer {
    fn module_deps(
        &self,
        path: &Path,
    ) -> Result<Vec<lang_core::ModuleDep>, Box<dyn std::error::Error + Send + Sync>> {
        let (module_deps, _classes) = python_imports::extract(path)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(module_deps)
    }
}

impl lang_core::ObjectAnalyzer for PythonAnalyzer {
    fn object_defs(
        &self,
        path: &Path,
    ) -> Result<Vec<lang_core::ClassDef>, Box<dyn std::error::Error + Send + Sync>> {
        let (_module_deps, classes) = python_imports::extract(path)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(classes)
    }
}

impl lang_core::ModuleNamer for PythonAnalyzer {
    fn file_extension(&self) -> &'static str {
        "py"
    }

    fn path_to_module_name(&self, rel_path: &Path) -> lang_core::ModuleName {
        let dotted = python_imports::path_to_module_name(rel_path);
        lang_core::ModuleName::new(dotted)
    }
}
