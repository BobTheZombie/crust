use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "crust", about = "Meson-like build system CLI", version, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Configure the project before building
    Configure,
    /// Build the project artifacts
    Build,
    /// Run the project tests
    Test,
    /// Clean generated build outputs
    Clean,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Configure => println!("Running configure step..."),
        Commands::Build => println!("Building project..."),
        Commands::Test => println!("Running tests..."),
        Commands::Clean => println!("Cleaning build outputs..."),
    }
}
