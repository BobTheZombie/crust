mod backend;
mod config;
mod graph;

use anyhow::{Context, Result};
use backend::{make::MakeBackend, ninja::NinjaBackend, Backend};
use clap::{Args, Parser, Subcommand, ValueEnum};
use config::ProjectManifest;
use graph::DependencyGraph;
use std::path::PathBuf;

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
    #[arg(long, value_enum, default_value = "ninja")]
    backend: BackendChoice,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum BackendChoice {
    Ninja,
    Make,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Configure(opts) => configure(&opts),
        Commands::Build(opts) => build(&opts),
        Commands::Test(opts) => build(&opts),
        Commands::Clean { builddir } => clean(&builddir),
    }
}

fn configure(opts: &CommandOptions) -> Result<()> {
    let manifest = ProjectManifest::load(&opts.manifest)?;
    let graph = DependencyGraph::from_manifest(&manifest)?;
    let backend = backend_from_choice(opts.backend);
    let expected_output = expected_backend_output(&*backend, &opts.builddir);

    if !graph.is_outdated(&opts.manifest, &[expected_output.clone()])? {
        println!(
            "{} backend already up-to-date at {}",
            backend.name(),
            expected_output.display()
        );
        return Ok(());
    }

    let result = backend.emit(&graph, &opts.builddir)?;
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
    Ok(())
}

fn build(opts: &CommandOptions) -> Result<()> {
    configure(opts)?;
    println!(
        "Backend ready. Invoke '{}' in {} to build.",
        opts.backend.command_hint(),
        opts.builddir.display()
    );
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

fn backend_from_choice(choice: BackendChoice) -> Box<dyn Backend> {
    match choice {
        BackendChoice::Ninja => Box::new(NinjaBackend),
        BackendChoice::Make => Box::new(MakeBackend),
    }
}

fn expected_backend_output(backend: &dyn Backend, builddir: &PathBuf) -> PathBuf {
    match backend.name() {
        "ninja" => builddir.join("build.ninja"),
        "make" => builddir.join("Makefile"),
        _ => builddir.join("backend"),
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
        }
    }
}
