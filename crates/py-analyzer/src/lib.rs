mod error;
mod model;
mod python_imports;

pub use error::InspectorError;
pub use lang_core::ModuleDep;

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
        let s = rel_path.to_string_lossy();
        let without_ext = s.strip_suffix(".py").unwrap_or(&s);
        let is_init = without_ext.ends_with("/__init__") || without_ext == "__init__";
        let dotted = if is_init {
            without_ext
                .strip_suffix("/__init__")
                .unwrap_or(without_ext)
                .replace('/', ".")
        } else {
            without_ext.replace('/', ".")
        };
        lang_core::ModuleName::new(dotted)
    }
}
