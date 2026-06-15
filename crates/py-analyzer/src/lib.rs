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
        let s = rel_path.to_string_lossy();
        let without_ext = s.trim_end_matches(".py");
        let dotted = without_ext.replace(['/', '\\'], ".");
        let name = dotted
            .strip_suffix(".__init__")
            .unwrap_or(&dotted)
            .to_string();
        lang_core::ModuleName::new(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_core::ModuleNamer as _;

    #[test]
    fn test_path_to_module_name_init_file_should_resolve_to_package_name() {
        let root = std::path::Path::new("/repo");
        let file = std::path::Path::new("/repo/myapp/domain/__init__.py");
        let rel = file.strip_prefix(root).unwrap_or(file);
        assert_eq!(
            PythonAnalyzer.path_to_module_name(rel).as_str(),
            "myapp.domain"
        );
    }

    #[test]
    fn test_path_to_module_name_regular_file_should_resolve_to_dotted_path() {
        let root = std::path::Path::new("/repo");
        let file = std::path::Path::new("/repo/myapp/domain/order.py");
        let rel = file.strip_prefix(root).unwrap_or(file);
        assert_eq!(
            PythonAnalyzer.path_to_module_name(rel).as_str(),
            "myapp.domain.order"
        );
    }
}
