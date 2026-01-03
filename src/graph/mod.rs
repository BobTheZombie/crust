use crate::config::{ProjectManifest, Target};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetKind {
    Executable,
    StaticLibrary,
    SharedLibrary,
    CustomCommand,
}

#[derive(Debug, Clone)]
pub struct TargetNode {
    pub name: String,
    pub kind: TargetKind,
    pub sources: Vec<String>,
    pub dependencies: Vec<String>,
    pub outputs: Vec<String>,
    pub command: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct DependencyGraph {
    nodes: HashMap<String, TargetNode>,
}

impl DependencyGraph {
    pub fn from_manifest(manifest: &ProjectManifest) -> Result<Self> {
        let mut graph = DependencyGraph {
            nodes: HashMap::new(),
        };

        for target in &manifest.targets {
            let name = target.name().to_string();
            if graph.nodes.contains_key(&name) {
                return Err(anyhow!("Duplicate target name: {}", name));
            }

            let (kind, sources, outputs, command) = match target.clone() {
                Target::Executable { sources, .. } => {
                    (TargetKind::Executable, sources, vec![name.clone()], None)
                }
                Target::StaticLibrary { sources, .. } => (
                    TargetKind::StaticLibrary,
                    sources,
                    vec![format!("lib{name}.a")],
                    None,
                ),
                Target::SharedLibrary { sources, .. } => (
                    TargetKind::SharedLibrary,
                    sources,
                    vec![format!("lib{name}.so")],
                    None,
                ),
                Target::CustomCommand {
                    outputs,
                    inputs,
                    command,
                    ..
                } => (TargetKind::CustomCommand, inputs, outputs, Some(command)),
            };

            let dependencies = target.dependencies().to_vec();
            graph.nodes.insert(
                name.clone(),
                TargetNode {
                    name,
                    kind,
                    sources,
                    dependencies,
                    outputs,
                    command,
                },
            );
        }

        graph.validate_dependencies()?;
        graph.check_cycles()?;

        Ok(graph)
    }

    fn validate_dependencies(&self) -> Result<()> {
        for node in self.nodes.values() {
            for dep in &node.dependencies {
                if !self.nodes.contains_key(dep) {
                    return Err(anyhow!(
                        "Unknown dependency '{}' referenced by '{}'",
                        dep,
                        node.name
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_cycles(&self) -> Result<()> {
        fn visit(
            node: &str,
            graph: &DependencyGraph,
            temp: &mut HashSet<String>,
            perm: &mut HashSet<String>,
        ) -> Result<()> {
            if perm.contains(node) {
                return Ok(());
            }
            if !temp.insert(node.to_string()) {
                return Err(anyhow!("Cycle detected involving '{}'", node));
            }
            if let Some(target) = graph.nodes.get(node) {
                for dep in &target.dependencies {
                    visit(dep, graph, temp, perm)?;
                }
            }
            temp.remove(node);
            perm.insert(node.to_string());
            Ok(())
        }

        let mut temp = HashSet::new();
        let mut perm = HashSet::new();
        for node in self.nodes.keys() {
            visit(node, self, &mut temp, &mut perm)?;
        }
        Ok(())
    }

    pub fn topo_order(&self) -> Result<Vec<&TargetNode>> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

        for (name, node) in &self.nodes {
            in_degree.insert(name.as_str(), node.dependencies.len());
            for dep in &node.dependencies {
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(name.as_str());
            }
        }

        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter_map(|(name, degree)| if *degree == 0 { Some(*name) } else { None })
            .collect();
        let mut result = Vec::new();

        while let Some(name) = queue.pop() {
            let node = self
                .nodes
                .get(name)
                .ok_or_else(|| anyhow!("Missing node {} during topo sort", name))?;
            result.push(node);
            if let Some(children) = dependents.get(name) {
                for child in children {
                    if let Some(degree) = in_degree.get_mut(child) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push(child);
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err(anyhow!("Cycle detected while performing topological sort"));
        }
        Ok(result)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &TargetNode> {
        self.nodes.values()
    }

    pub fn is_outdated(&self, manifest_path: &Path, backend_outputs: &[PathBuf]) -> Result<bool> {
        if backend_outputs.is_empty() {
            return Ok(true);
        }
        for output in backend_outputs {
            if !output.exists() {
                return Ok(true);
            }
        }

        let manifest_meta = fs::metadata(manifest_path)?;
        let manifest_mtime = manifest_meta.modified()?;
        let manifest_dir = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let latest_input = self.latest_input_time(&manifest_dir, manifest_mtime)?;
        let oldest_output = self.oldest_output_time(backend_outputs)?;
        Ok(latest_input > oldest_output)
    }

    fn latest_input_time(&self, manifest_dir: &Path, initial: SystemTime) -> Result<SystemTime> {
        let mut latest = initial;
        for node in self.nodes.values() {
            for src in &node.sources {
                let path = manifest_dir.join(src);
                if path.exists() {
                    let time = fs::metadata(&path)?.modified()?;
                    if time > latest {
                        latest = time;
                    }
                }
            }
        }
        Ok(latest)
    }

    fn oldest_output_time(&self, outputs: &[PathBuf]) -> Result<SystemTime> {
        let mut oldest: Option<SystemTime> = None;
        for output in outputs {
            let meta = fs::metadata(output)?;
            let modified = meta.modified()?;
            oldest = Some(oldest.map_or(modified, |current| current.min(modified)));
        }
        oldest.ok_or_else(|| anyhow!("No outputs found for incremental check"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProjectInfo, Target};
    use std::io::Write;
    use tempfile::tempdir;

    fn sample_manifest() -> ProjectManifest {
        ProjectManifest {
            project: ProjectInfo {
                name: "demo".into(),
                version: None,
            },
            targets: vec![
                Target::StaticLibrary {
                    name: "core".into(),
                    sources: vec!["src/core.c".into()],
                    deps: vec![],
                },
                Target::Executable {
                    name: "app".into(),
                    sources: vec!["src/main.c".into()],
                    deps: vec!["core".into()],
                },
            ],
        }
    }

    #[test]
    fn builds_graph_and_topo_sort() {
        let manifest = sample_manifest();
        let graph = DependencyGraph::from_manifest(&manifest).unwrap();
        let names: Vec<_> = graph
            .topo_order()
            .unwrap()
            .iter()
            .map(|n| n.name.clone())
            .collect();
        assert_eq!(names, vec!["core", "app"]);
    }

    #[test]
    fn detects_cycles() {
        let manifest = ProjectManifest {
            project: ProjectInfo {
                name: "demo".into(),
                version: None,
            },
            targets: vec![Target::Executable {
                name: "app".into(),
                sources: vec!["src/main.c".into()],
                deps: vec!["app".into()],
            }],
        };
        let result = DependencyGraph::from_manifest(&manifest);
        assert!(result.is_err());
    }

    #[test]
    fn incremental_detection_checks_sources() {
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("crust.build");
        let mut manifest_file = std::fs::File::create(&manifest_path).unwrap();
        manifest_file
            .write_all(
                br#"[project]
name = "demo"

[[targets]]
type = "executable"
name = "app"
sources = ["src/main.c"]
"#,
            )
            .unwrap();

        let manifest = ProjectManifest::load(&manifest_path).unwrap();
        let graph = DependencyGraph::from_manifest(&manifest).unwrap();

        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let source_path = src_dir.join("main.c");
        std::fs::write(&source_path, "int main() { return 0; }").unwrap();

        let backend_out = dir.path().join("build.ninja");
        std::fs::write(&backend_out, "# backend").unwrap();

        assert!(!graph
            .is_outdated(&manifest_path, &[backend_out.clone()])
            .unwrap());

        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&source_path, "int main() { return 1; }").unwrap();

        assert!(graph.is_outdated(&manifest_path, &[backend_out]).unwrap());
    }
}
