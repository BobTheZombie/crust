use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Target {
    #[serde(rename = "executable")]
    Executable {
        name: String,
        sources: Vec<String>,
        #[serde(default)]
        deps: Vec<String>,
    },
    #[serde(rename = "static_library")]
    StaticLibrary {
        name: String,
        sources: Vec<String>,
        #[serde(default)]
        deps: Vec<String>,
    },
    #[serde(rename = "shared_library")]
    SharedLibrary {
        name: String,
        sources: Vec<String>,
        #[serde(default)]
        deps: Vec<String>,
    },
    #[serde(rename = "custom_command")]
    CustomCommand {
        name: String,
        command: String,
        outputs: Vec<String>,
        #[serde(default)]
        deps: Vec<String>,
        #[serde(default)]
        inputs: Vec<String>,
    },
}

impl Target {
    pub fn name(&self) -> &str {
        match self {
            Target::Executable { name, .. }
            | Target::StaticLibrary { name, .. }
            | Target::SharedLibrary { name, .. }
            | Target::CustomCommand { name, .. } => name,
        }
    }

    pub fn dependencies(&self) -> &[String] {
        match self {
            Target::Executable { deps, .. }
            | Target::StaticLibrary { deps, .. }
            | Target::SharedLibrary { deps, .. }
            | Target::CustomCommand { deps, .. } => deps,
        }
    }

    pub fn sources(&self) -> &[String] {
        match self {
            Target::Executable { sources, .. }
            | Target::StaticLibrary { sources, .. }
            | Target::SharedLibrary { sources, .. } => sources,
            Target::CustomCommand { inputs, .. } => inputs,
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct ProjectManifest {
    pub project: ProjectInfo,
    #[serde(default)]
    pub targets: Vec<Target>,
}

impl ProjectManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest at {}", path.display()))?;
        let manifest: ProjectManifest = toml::from_str(&content)
            .with_context(|| format!("Invalid manifest TOML at {}", path.display()))?;
        Ok(manifest)
    }

    pub fn manifest_dir(manifest_path: &Path) -> PathBuf {
        manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn parses_manifest_with_multiple_target_types() {
        let mut file = NamedTempFile::new().unwrap();
        let contents = r#"
[project]
name = "demo"
version = "1.0"

[[targets]]
type = "executable"
name = "app"
sources = ["src/main.c"]
deps = ["util"]

[[targets]]
type = "static_library"
name = "util"
sources = ["src/util.c"]

[[targets]]
type = "custom_command"
name = "codegen"
command = "python gen.py"
outputs = ["generated.h"]
inputs = ["schema.json"]
"#;
        std::io::Write::write_all(&mut file, contents.as_bytes()).unwrap();

        let manifest = ProjectManifest::load(file.path()).unwrap();
        assert_eq!(manifest.project.name, "demo");
        assert_eq!(manifest.targets.len(), 3);
        assert_eq!(manifest.targets[0].name(), "app");
    }
}
