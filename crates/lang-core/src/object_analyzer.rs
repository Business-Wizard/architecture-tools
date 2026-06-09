use std::path::Path;

use crate::ClassDef;

pub trait ObjectAnalyzer {
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    fn object_defs(
        &self,
        path: &Path,
    ) -> Result<Vec<ClassDef>, Box<dyn std::error::Error + Send + Sync>>;
}
