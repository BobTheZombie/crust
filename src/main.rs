mod backend;
mod config;
mod graph;

use anyhow::{Context, Result};
use backend::{make::MakeBackend, native::CrustBackend, ninja::NinjaBackend, Backend};
use clap::{Args, Parser, Subcommand, ValueEnum};
use config::ProjectManifest;
use graph::DependencyGraph;
use std::path::{Path, PathBuf};

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
    let backend = backend_from_choice(opts.backend, &manifest_dir);
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
        let result = backend.emit(&graph, &opts.builddir, &manifest_dir)?;
        if backend.name() == "native" {
            println!(
                "Built {} output(s) in {}",
                result.files.len(),
                opts.builddir.display()
            );
        } else {
            println!(
                "Generated {} backend files: {}",
                backend.name(),
                result
                    .files
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
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

fn backend_from_choice(choice: BackendChoice, manifest_dir: &Path) -> Box<dyn Backend> {
    match choice {
        BackendChoice::Native => Box::new(CrustBackend::new(manifest_dir.to_path_buf())),
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
