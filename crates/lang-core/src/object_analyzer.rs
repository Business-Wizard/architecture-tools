use std::path::Path;

use crate::ClassDef;

pub trait ObjectAnalyzer {
    fn object_defs(
        &self,
        path: &Path,
    ) -> Result<Vec<ClassDef>, Box<dyn std::error::Error + Send + Sync>>;
}
