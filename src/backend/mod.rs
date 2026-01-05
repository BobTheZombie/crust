use crate::graph::DependencyGraph;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub mod make;
pub mod native;
pub mod ninja;

pub trait Backend {
    fn name(&self) -> &str;
    fn emit(
        &self,
        graph: &DependencyGraph,
        out_dir: &Path,
        manifest_dir: &Path,
    ) -> Result<BackendEmitResult>;

    fn primary_outputs(&self, graph: &DependencyGraph, out_dir: &Path) -> Vec<PathBuf> {
        let _ = graph;
        let _ = out_dir;
        Vec::new()
    }
}

#[derive(Debug, Clone)]
pub struct BackendEmitResult {
    pub files: Vec<PathBuf>,
}

impl BackendEmitResult {
    pub fn single(path: PathBuf) -> Self {
        BackendEmitResult { files: vec![path] }
    }
}
