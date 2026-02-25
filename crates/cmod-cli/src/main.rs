mod commands;

use clap::{Parser, Subcommand};


#[derive(Parser)]
#[command(
    name = "cmod",
    about = "Cargo-inspired, Git-native package and build tool for C++20 modules",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Use the lockfile strictly; fail if it is outdated
    #[arg(long, global = true)]
    locked: bool,

    /// Disable network access
    #[arg(long, global = true)]
    offline: bool,

    /// Enable verbose output
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Override the target triple
    #[arg(long, global = true)]
    target: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new module or workspace
    Init {
        /// Initialize as a workspace instead of a single module
        #[arg(long)]
        workspace: bool,

        /// Project name (defaults to directory name)
        #[arg(long)]
        name: Option<String>,
    },

    /// Add a dependency
    Add {
        /// Dependency specifier (e.g., github.com/fmtlib/fmt or github.com/fmtlib/fmt@^10.2)
        dep: String,

        /// Git URL (if different from the key)
        #[arg(long)]
        git: Option<String>,

        /// Git branch
        #[arg(long)]
        branch: Option<String>,

        /// Exact Git revision
        #[arg(long)]
        rev: Option<String>,

        /// Path dependency
        #[arg(long)]
        path: Option<String>,

        /// Features to enable
        #[arg(long, value_delimiter = ',')]
        features: Vec<String>,
    },

    /// Remove a dependency
    Remove {
        /// Name of the dependency to remove
        name: String,
    },

    /// Resolve dependencies and generate/update the lockfile
    Resolve,

    /// Build the current module or workspace
    Build {
        /// Build in release mode
        #[arg(long)]
        release: bool,
    },

    /// Run module tests
    Test {
        /// Build in release mode
        #[arg(long)]
        release: bool,
    },

    /// Update dependencies
    Update {
        /// Specific dependency to update
        name: Option<String>,
    },

    /// Inspect the dependency graph
    Deps {
        /// Display as a tree
        #[arg(long)]
        tree: bool,
    },

    /// Manage the build cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Verify integrity and security
    Verify,
}

#[derive(Subcommand)]
enum CacheAction {
    /// Show cache status and size
    Status,
    /// Clear the local cache
    Clean,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { workspace, name } => commands::init::run(workspace, name),
        Commands::Add {
            dep,
            git,
            branch,
            rev,
            path,
            features,
        } => commands::add::run(dep, git, branch, rev, path, features, cli.locked, cli.offline),
        Commands::Remove { name } => commands::remove::run(name),
        Commands::Resolve => commands::resolve::run(cli.locked, cli.offline, cli.verbose),
        Commands::Build { release } => {
            commands::build::run(release, cli.locked, cli.offline, cli.verbose, cli.target)
        }
        Commands::Test { release } => {
            commands::test::run(release, cli.locked, cli.offline, cli.verbose, cli.target)
        }
        Commands::Update { name } => commands::update::run(name, cli.verbose),
        Commands::Deps { tree } => commands::deps::run(tree),
        Commands::Cache { action } => match action {
            CacheAction::Status => commands::cache::status(),
            CacheAction::Clean => commands::cache::clean(),
        },
        Commands::Verify => commands::verify::run(cli.verbose),
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(e.exit_code());
    }
}
