mod backend;
mod config;
mod executor;
mod graph;

use anyhow::{Context, Result};
use backend::{
    make::MakeBackend, native::CrustBackend, ninja::NinjaBackend, Backend, BackendEmitResult,
    TargetBuildSummary,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use config::ProjectManifest;
use graph::DependencyGraph;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "crust",
    about = "Meson-like build system CLI",
    version,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Configure the project before building
    Configure(CommandOptions),
    /// Build the project artifacts
    Build(CommandOptions),
    /// Run the project tests
    Test(CommandOptions),
    /// Clean generated build outputs
    Clean {
        #[arg(short = 'b', long, default_value = "build")]
        builddir: PathBuf,
    },
}

#[derive(Clone, Debug, Args)]
struct CommandOptions {
    /// Path to the crust manifest (TOML)
    #[arg(long, default_value = "crust.build")]
    manifest: PathBuf,

    /// Output directory for generated build files
    #[arg(short = 'b', long, default_value = "build")]
    builddir: PathBuf,

    /// Maximum number of concurrent jobs (defaults to CPU count)
    #[arg(short = 'j', long)]
    jobs: Option<usize>,

    /// Backend used to generate build files
    #[arg(long, value_enum, default_value_t = BackendChoice::Native)]
    backend: BackendChoice,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum BackendChoice {
    Native,
    Ninja,
    Make,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Configure(opts) => drive(&opts, false),
        Commands::Build(opts) => drive(&opts, true),
        Commands::Test(opts) => drive(&opts, true),
        Commands::Clean { builddir } => clean(&builddir),
    }
}

fn drive(opts: &CommandOptions, show_hint: bool) -> Result<()> {
    let manifest = ProjectManifest::load(&opts.manifest)?;
    let graph = DependencyGraph::from_manifest(&manifest)?;
    let manifest_dir = ProjectManifest::manifest_dir(&opts.manifest);
    if let Some(0) = opts.jobs {
        return Err(anyhow::anyhow!("--jobs must be at least 1"));
    }
    let backend = backend_from_choice(opts.backend, &manifest_dir, opts.jobs);
    let outputs_to_check = backend.primary_outputs(&graph, &opts.builddir);
    let outdated =
        outputs_to_check.is_empty() || graph.is_outdated(&opts.manifest, &outputs_to_check)?;

    if !outdated {
        println!(
            "{} backend already up-to-date at {}",
            backend.name(),
            opts.builddir.display()
        );
    } else {
        let emit_start = Instant::now();
        let mut result = backend.emit(&graph, &opts.builddir, &manifest_dir)?;
        let total_elapsed = emit_start.elapsed();

        if result.target_summaries.is_empty() {
            result.target_summaries = graph
                .topo_order()?
                .into_iter()
                .map(|node| backend_summary_from_graph(node, &opts.builddir))
                .collect();
        }

        print_summary(backend.as_ref(), &result, total_elapsed);
    }

    if show_hint {
        if backend.name() == "native" {
            println!(
                "Native build complete. Outputs live in {}",
                opts.builddir.display()
            );
        } else {
            println!(
                "Backend ready. Invoke '{}' in {} to build.",
                opts.backend.command_hint(),
                opts.builddir.display()
            );
        }
    }

    Ok(())
}

fn backend_summary_from_graph(node: &graph::TargetNode, builddir: &Path) -> TargetBuildSummary {
    TargetBuildSummary {
        name: node.name.clone(),
        built: false,
        outputs: node.outputs.iter().map(|o| builddir.join(o)).collect(),
        duration: Duration::default(),
    }
}

fn print_summary(backend: &dyn Backend, result: &BackendEmitResult, total_elapsed: Duration) {
    let built_count = result.target_summaries.iter().filter(|t| t.built).count();
    let skipped_count = result.target_summaries.len().saturating_sub(built_count);

    println!("\nBuild summary");
    println!("  Backend: {}", backend.name());
    println!(
        "  Targets: {} built, {} skipped, {} total",
        built_count,
        skipped_count,
        result.target_summaries.len()
    );
    println!("  Elapsed time: {}", format_duration(total_elapsed));

    if !result.files.is_empty() {
        println!("  Backend outputs:");
        for file in &result.files {
            println!("    - {}", file.display());
        }
    }

    if !result.target_summaries.is_empty() {
        println!("  Target results:");
        for target in &result.target_summaries {
            let status = if target.built { "built" } else { "skipped" };
            println!(
                "    - {} ({status}, {})",
                target.name,
                format_duration(target.duration)
            );
            for output in &target.outputs {
                println!("      -> {}", output.display());
            }
        }
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{:.2}s", duration.as_secs_f64())
}

fn clean(builddir: &PathBuf) -> Result<()> {
    if builddir.exists() {
        std::fs::remove_dir_all(builddir)
            .with_context(|| format!("Failed to remove {}", builddir.display()))?;
        println!("Removed {}", builddir.display());
    } else {
        println!("Nothing to clean");
    }
    Ok(())
}

fn backend_from_choice(
    choice: BackendChoice,
    manifest_dir: &Path,
    jobs: Option<usize>,
) -> Box<dyn Backend> {
    match choice {
        BackendChoice::Native => Box::new(CrustBackend::new(manifest_dir.to_path_buf(), jobs)),
        BackendChoice::Ninja => Box::new(NinjaBackend),
        BackendChoice::Make => Box::new(MakeBackend),
    }
}

trait BackendHint {
    fn command_hint(&self) -> &'static str;
}

impl BackendHint for BackendChoice {
    fn command_hint(&self) -> &'static str {
        match self {
            BackendChoice::Ninja => "ninja",
            BackendChoice::Make => "make",
            BackendChoice::Native => "native",
        }
    }
}
