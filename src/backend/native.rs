use crate::backend::{Backend, BackendEmitResult};
use crate::executor::BuildExecutor;
use crate::graph::{DependencyGraph, TargetKind};
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[derive(Clone)]
pub struct CrustBackend {
    manifest_dir: PathBuf,
    parallelism: Option<usize>,
}

impl CrustBackend {
    pub fn new(manifest_dir: PathBuf, parallelism: Option<usize>) -> Self {
        CrustBackend {
            manifest_dir,
            parallelism,
        }
    }

    fn needs_rebuild(&self, inputs: &[PathBuf], outputs: &[PathBuf]) -> Result<bool> {
        if outputs.is_empty() {
            return Ok(true);
        }

        for output in outputs {
            if !output.exists() {
                return Ok(true);
            }
        }

        let latest_input = self.latest_mod_time(inputs)?;
        let oldest_output = self.oldest_mod_time(outputs)?;
        Ok(latest_input > oldest_output)
    }

    fn latest_mod_time(&self, paths: &[PathBuf]) -> Result<SystemTime> {
        let mut latest = SystemTime::UNIX_EPOCH;
        for path in paths {
            if path.exists() {
                let modified = fs::metadata(path)?.modified()?;
                latest = latest.max(modified);
            }
        }
        Ok(latest)
    }

    fn oldest_mod_time(&self, paths: &[PathBuf]) -> Result<SystemTime> {
        let mut oldest: Option<SystemTime> = None;
        for path in paths {
            let modified = fs::metadata(path)?.modified()?;
            oldest = Some(oldest.map_or(modified, |current| current.min(modified)));
        }
        oldest.ok_or_else(|| anyhow!("No paths provided for modification time check"))
    }

    fn compile_objects(
        &self,
        sources: &[String],
        out_dir: &Path,
        target_name: &str,
    ) -> Result<Vec<PathBuf>> {
        let mut objects = Vec::new();
        for (idx, source) in sources.iter().enumerate() {
            let source_path = self.manifest_dir.join(source);
            let object_path = out_dir.join(format!("{target_name}_{idx}.o"));
            if !self.needs_rebuild(&[source_path.clone()], &[object_path.clone()])? {
                objects.push(object_path.clone());
                continue;
            }

            if let Some(parent) = object_path.parent() {
                fs::create_dir_all(parent)?;
            }

            println!(
                "Compiling {} -> {}",
                source_path.display(),
                object_path.display()
            );
            let status = Command::new("cc")
                .arg("-c")
                .arg(&source_path)
                .arg("-o")
                .arg(&object_path)
                .status()
                .with_context(|| format!("Failed to spawn compiler for {}", source))?;
            if !status.success() {
                return Err(anyhow!("Compilation failed for {}", source));
            }
            objects.push(object_path);
        }
        Ok(objects)
    }

    fn run_custom_command(
        &self,
        command: &str,
        inputs: &[PathBuf],
        outputs: &[PathBuf],
        out_dir: &Path,
    ) -> Result<()> {
        for output in outputs {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
        }

        if !self.needs_rebuild(inputs, outputs)? {
            return Ok(());
        }

        println!("Running custom command: {}", command);
        let status = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.manifest_dir)
            .env("CRUST_BUILDDIR", out_dir)
            .status()
            .context("Failed to spawn custom command")?;
        if !status.success() {
            return Err(anyhow!("Custom command failed: {}", command));
        }

        for output in outputs {
            if output.exists() {
                continue;
            }

            let manifest_output = self
                .manifest_dir
                .join(output.strip_prefix(out_dir).unwrap_or(output));
            if manifest_output.exists() {
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&manifest_output, output).with_context(|| {
                    format!(
                        "Failed to copy {} to {}",
                        manifest_output.display(),
                        output.display()
                    )
                })?;
            }
        }

        Ok(())
    }

    fn link_executable(
        &self,
        name: &str,
        sources: &[String],
        dep_outputs: &[PathBuf],
        out_dir: &Path,
    ) -> Result<PathBuf> {
        let outputs = vec![out_dir.join(name)];
        if !self.needs_rebuild(&self.collect_inputs(sources, dep_outputs), &outputs)? {
            return Ok(outputs[0].clone());
        }

        let objects = self.compile_objects(sources, out_dir, name)?;
        let mut cmd = Command::new("cc");
        cmd.arg("-o").arg(&outputs[0]);
        for obj in &objects {
            cmd.arg(obj);
        }
        for dep in dep_outputs {
            cmd.arg(dep);
        }

        println!("Linking executable {}", outputs[0].display());
        let status = cmd.status().context("Failed to spawn linker")?;
        if !status.success() {
            return Err(anyhow!("Linking failed for executable {}", name));
        }

        Ok(outputs[0].clone())
    }

    fn link_shared_library(
        &self,
        name: &str,
        sources: &[String],
        dep_outputs: &[PathBuf],
        out_dir: &Path,
    ) -> Result<PathBuf> {
        let output = out_dir.join(format!("lib{name}.so"));
        if !self.needs_rebuild(
            &self.collect_inputs(sources, dep_outputs),
            &[output.clone()],
        )? {
            return Ok(output);
        }

        let objects = self.compile_objects(sources, out_dir, name)?;
        let mut cmd = Command::new("cc");
        cmd.arg("-shared").arg("-o").arg(&output);
        for obj in &objects {
            cmd.arg(obj);
        }
        for dep in dep_outputs {
            cmd.arg(dep);
        }

        println!("Linking shared library {}", output.display());
        let status = cmd.status().context("Failed to spawn shared linker")?;
        if !status.success() {
            return Err(anyhow!("Linking failed for shared library {}", name));
        }

        Ok(output)
    }

    fn archive_static_library(
        &self,
        name: &str,
        sources: &[String],
        dep_outputs: &[PathBuf],
        out_dir: &Path,
    ) -> Result<PathBuf> {
        let output = out_dir.join(format!("lib{name}.a"));
        let inputs = self.collect_inputs(sources, dep_outputs);
        if !self.needs_rebuild(&inputs, &[output.clone()])? {
            return Ok(output);
        }

        let objects = self.compile_objects(sources, out_dir, name)?;
        let mut cmd = Command::new("ar");
        cmd.arg("rcs").arg(&output);
        for obj in &objects {
            cmd.arg(obj);
        }

        println!("Archiving static library {}", output.display());
        let status = cmd.status().context("Failed to spawn archiver")?;
        if !status.success() {
            return Err(anyhow!("Archiving failed for static library {}", name));
        }

        Ok(output)
    }

    fn collect_inputs(&self, sources: &[String], dep_outputs: &[PathBuf]) -> Vec<PathBuf> {
        let mut inputs: Vec<PathBuf> = sources.iter().map(|s| self.manifest_dir.join(s)).collect();
        inputs.extend_from_slice(dep_outputs);
        inputs
    }

    fn execute_target(
        &self,
        node: &crate::graph::TargetNode,
        dep_outputs: &[PathBuf],
        out_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        let outputs: Vec<PathBuf> = node.outputs.iter().map(|o| out_dir.join(o)).collect();

        match node.kind {
            TargetKind::Executable => {
                let output =
                    self.link_executable(&node.name, &node.sources, dep_outputs, out_dir)?;
                Ok(vec![output])
            }
            TargetKind::StaticLibrary => {
                let output =
                    self.archive_static_library(&node.name, &node.sources, dep_outputs, out_dir)?;
                Ok(vec![output])
            }
            TargetKind::SharedLibrary => {
                let output =
                    self.link_shared_library(&node.name, &node.sources, dep_outputs, out_dir)?;
                Ok(vec![output])
            }
            TargetKind::CustomCommand => {
                let inputs = self.collect_inputs(&node.sources, dep_outputs);
                self.run_custom_command(
                    node.command
                        .as_deref()
                        .ok_or_else(|| anyhow!("Missing custom command for {}", node.name))?,
                    &inputs,
                    &outputs,
                    out_dir,
                )?;
                Ok(outputs)
            }
        }
    }
}

impl Backend for CrustBackend {
    fn name(&self) -> &str {
        "native"
    }

    fn emit(
        &self,
        graph: &DependencyGraph,
        out_dir: &Path,
        _manifest_dir: &Path,
    ) -> Result<BackendEmitResult> {
        fs::create_dir_all(out_dir)?;
        let executor = BuildExecutor::new(self.parallelism);
        let out_dir = out_dir.to_path_buf();
        let backend = self.clone();

        let result = executor.execute(graph, move |node, dep_outputs| {
            backend.execute_target(node, &dep_outputs, &out_dir)
        })?;

        let all_outputs: Vec<PathBuf> = result
            .produced
            .values()
            .flat_map(|outputs| outputs.iter().cloned())
            .collect();

        Ok(BackendEmitResult { files: all_outputs })
    }

    fn primary_outputs(&self, graph: &DependencyGraph, out_dir: &Path) -> Vec<PathBuf> {
        graph
            .nodes()
            .flat_map(|n| n.outputs.iter().map(|o| out_dir.join(o)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProjectManifest;
    use tempfile::tempdir;

    #[test]
    fn builds_executable_native() {
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("crust.build");
        fs::write(
            &manifest_path,
            r#"[project]
name = "demo"

[[targets]]
type = "executable"
name = "app"
sources = ["main.c"]
"#,
        )
        .unwrap();
        fs::write(dir.path().join("main.c"), "int main(){return 0;}").unwrap();

        let manifest = ProjectManifest::load(&manifest_path).unwrap();
        let graph = DependencyGraph::from_manifest(&manifest).unwrap();
        let builddir = dir.path().join("build");
        let backend = CrustBackend::new(dir.path().to_path_buf(), None);

        let result = backend.emit(&graph, &builddir, dir.path()).unwrap();
        let output = &result.files[0];
        assert!(output.exists());
        assert!(output.ends_with("app"));
    }
}
