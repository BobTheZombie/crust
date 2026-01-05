use crate::graph::{DependencyGraph, TargetNode};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct ExecutionResult {
    pub produced: HashMap<String, Vec<std::path::PathBuf>>,
}

#[derive(Debug, Clone, Copy)]
pub struct BuildExecutor {
    workers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProjectInfo, ProjectManifest, Target};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    #[test]
    fn schedules_dependencies_before_dependents() {
        let manifest = ProjectManifest {
            project: ProjectInfo {
                name: "demo".into(),
                version: None,
            },
            targets: vec![
                Target::CustomCommand {
                    name: "prep".into(),
                    command: "touch a".into(),
                    outputs: vec!["a".into()],
                    deps: vec![],
                    inputs: vec![],
                },
                Target::CustomCommand {
                    name: "gen".into(),
                    command: "touch b".into(),
                    outputs: vec!["b".into()],
                    deps: vec!["prep".into()],
                    inputs: vec![],
                },
                Target::CustomCommand {
                    name: "assemble".into(),
                    command: "touch c".into(),
                    outputs: vec!["c".into()],
                    deps: vec!["gen".into()],
                    inputs: vec![],
                },
            ],
        };

        let graph = DependencyGraph::from_manifest(&manifest).unwrap();
        let executor = BuildExecutor::new(Some(2));
        let completed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let result = executor
            .execute(&graph, move |node, _| {
                let mut done = completed.lock().unwrap();
                for dep in &node.dependencies {
                    assert!(done.contains(dep), "dependency {} not complete", dep);
                }
                done.push(node.name.clone());
                Ok(node.outputs.iter().map(|o| PathBuf::from(o)).collect())
            })
            .unwrap();

        assert_eq!(result.produced.len(), 3);
    }
}

impl BuildExecutor {
    pub fn new(parallelism: Option<usize>) -> Self {
        let workers = parallelism.unwrap_or_else(|| num_cpus::get().max(1));
        BuildExecutor { workers }
    }

    pub fn execute<F>(&self, graph: &DependencyGraph, run_node: F) -> Result<ExecutionResult>
    where
        F: Fn(&TargetNode, Vec<std::path::PathBuf>) -> Result<Vec<std::path::PathBuf>>
            + Send
            + Sync
            + 'static,
    {
        let nodes: HashMap<String, TargetNode> =
            graph.nodes().map(|n| (n.name.clone(), n.clone())).collect();

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        for (name, node) in &nodes {
            in_degree.insert(name.clone(), node.dependencies.len());
            for dep in &node.dependencies {
                dependents
                    .entry(dep.clone())
                    .or_default()
                    .push(name.clone());
            }
        }

        let ready: VecDeque<String> = in_degree
            .iter()
            .filter_map(|(name, degree)| {
                if *degree == 0 {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();

        let produced: Arc<Mutex<HashMap<String, Vec<std::path::PathBuf>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let nodes = Arc::new(nodes);
        let (task_tx, task_rx) = crossbeam_channel::unbounded::<String>();
        let (done_tx, done_rx) = crossbeam_channel::unbounded();
        let run_node = Arc::new(run_node);

        let mut handles = Vec::new();
        for _ in 0..self.workers {
            let task_rx = task_rx.clone();
            let done_tx = done_tx.clone();
            let nodes = Arc::clone(&nodes);
            let produced = Arc::clone(&produced);
            let run_node = Arc::clone(&run_node);
            handles.push(thread::spawn(move || {
                while let Ok(name) = task_rx.recv() {
                    let node = match nodes.get(&name) {
                        Some(node) => node,
                        None => {
                            let _ = done_tx.send((name, Err(anyhow!("Unknown node"))));
                            continue;
                        }
                    };
                    let dep_outputs: Vec<_> = {
                        let map = produced.lock().expect("produced mutex poisoned");
                        node.dependencies
                            .iter()
                            .flat_map(|d| map.get(d).cloned().unwrap_or_default())
                            .collect()
                    };
                    let result = run_node(node, dep_outputs);
                    let _ = done_tx.send((name, result));
                }
            }));
        }

        drop(done_tx);
        for name in ready {
            task_tx
                .send(name)
                .map_err(|e| anyhow!("Failed to enqueue task: {}", e))?;
        }

        let total = nodes.len();
        let mut remaining = total;
        let mut in_degree = in_degree;
        let mut dependents = dependents;
        let mut first_error: Option<anyhow::Error> = None;

        while remaining > 0 {
            let (name, result) = match done_rx.recv() {
                Ok(msg) => msg,
                Err(err) => {
                    first_error = Some(anyhow!("Executor stopped unexpectedly: {}", err));
                    break;
                }
            };

            match result {
                Ok(outputs) => {
                    produced
                        .lock()
                        .expect("produced mutex poisoned")
                        .insert(name.clone(), outputs);

                    if let Some(children) = dependents.remove(&name) {
                        for child in children {
                            if let Some(degree) = in_degree.get_mut(&child) {
                                if *degree > 0 {
                                    *degree -= 1;
                                }
                                if *degree == 0 {
                                    task_tx
                                        .send(child.clone())
                                        .map_err(|e| anyhow!("Failed to enqueue task: {}", e))?;
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    first_error = Some(err);
                    break;
                }
            }

            remaining -= 1;
        }

        drop(task_tx);
        for handle in handles {
            if let Err(join_err) = handle.join() {
                return Err(anyhow!("Worker thread panicked: {:?}", join_err));
            }
        }

        if let Some(err) = first_error {
            return Err(err);
        }

        let produced = Arc::into_inner(produced)
            .unwrap_or_default()
            .into_inner()
            .unwrap_or_default();

        if produced.len() != total {
            return Err(anyhow!(
                "Build did not complete: expected {} nodes, finished {}",
                total,
                produced.len()
            ));
        }

        Ok(ExecutionResult { produced })
    }
}
