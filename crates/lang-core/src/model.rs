use std::ops::Deref;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModuleName(String);

impl ModuleName {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for ModuleName {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ModuleName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ModuleName {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for ModuleName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileExtension(pub &'static str);

impl FileExtension {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl AsRef<str> for FileExtension {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl std::fmt::Display for FileExtension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDep {
    pub from: ModuleName,
    pub to: ModuleName,
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
