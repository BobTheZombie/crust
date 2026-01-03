use crate::graph::DependencyGraph;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub mod make;
pub mod ninja;

pub trait Backend {
    fn name(&self) -> &str;
    fn emit(&self, graph: &DependencyGraph, out_dir: &Path) -> Result<BackendEmitResult>;
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
