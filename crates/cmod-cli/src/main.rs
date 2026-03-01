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

    /// Enable specific features (comma-separated)
    #[arg(long, global = true, value_delimiter = ',')]
    features: Vec<String>,

    /// Disable default features
    #[arg(long, global = true)]
    no_default_features: bool,

    /// Skip build cache
    #[arg(long, global = true)]
    no_cache: bool,

    /// Skip TOFU trust verification for dependencies
    #[arg(long, global = true)]
    untrusted: bool,
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

        /// Maximum parallel compilation jobs (0 = auto)
        #[arg(long, short, default_value = "0")]
        jobs: usize,

        /// Force rebuild, ignoring incremental state
        #[arg(long)]
        force: bool,

        /// Remote cache URL (overrides manifest [cache].shared_url)
        #[arg(long)]
        remote_cache: Option<String>,

        /// Skip pre-build and post-build hooks
        #[arg(long)]
        no_hooks: bool,
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

        /// Only allow patch-level updates (e.g., 1.2.3 → 1.2.4)
        #[arg(long)]
        patch: bool,
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
    Verify {
        /// Check commit signatures
        #[arg(long)]
        signatures: bool,
    },

    /// Visualize the module dependency graph
    Graph {
        /// Output format: ascii, dot, json
        #[arg(long, default_value = "ascii")]
        format: Option<String>,

        /// Filter modules matching this pattern
        #[arg(long)]
        filter: Option<String>,
    },

    /// Audit dependencies for security and quality issues
    Audit,

    /// Show project status overview
    Status,

    /// Explain why a module would be rebuilt
    Explain {
        /// Module name to explain
        module: String,
    },

    /// Manage the active toolchain
    Toolchain {
        #[command(subcommand)]
        action: ToolchainAction,
    },

    /// Vendor dependencies for offline builds
    Vendor {
        /// Re-synchronize vendored deps with lockfile
        #[arg(long)]
        sync: bool,
    },

    /// Lint C++ source files for common issues
    Lint,

    /// Format C++ source files using clang-format
    Fmt {
        /// Check formatting without modifying files
        #[arg(long)]
        check: bool,
    },

    /// Search for modules by name
    Search {
        /// Search query (substring match)
        query: String,
    },

    /// Build and run the project binary
    Run {
        /// Build in release mode
        #[arg(long)]
        release: bool,

        /// Arguments to pass to the binary
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Remove build artifacts
    Clean,

    /// Manage workspace members
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },

    /// Generate a Software Bill of Materials (SBOM)
    Sbom {
        /// Output file path (prints to stdout if not specified)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Publish a release (create a Git tag)
    Publish {
        /// Dry run — show what would happen without making changes
        #[arg(long)]
        dry_run: bool,

        /// Push the tag to origin after creation
        #[arg(long)]
        push: bool,
    },

    /// Generate compile_commands.json for IDE integration
    CompileCommands,
}

#[derive(Subcommand)]
enum WorkspaceAction {
    /// List workspace members
    List,
    /// Add a new member to the workspace
    Add {
        /// Name of the new member
        name: String,
    },
    /// Remove a member from the workspace
    Remove {
        /// Name of the member to remove
        name: String,
    },
}

#[derive(Subcommand)]
enum ToolchainAction {
    /// Show active toolchain configuration
    Show,
    /// Validate toolchain availability
    Check,
}

#[derive(Subcommand)]
enum CacheAction {
    /// Show cache status and size
    Status,
    /// Clear the local cache
    Clean,
    /// Push local cache entries to remote cache
    Push,
    /// Pull cache entries from remote cache
    Pull,
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
        Commands::Resolve => commands::resolve::run(
            cli.locked,
            cli.offline,
            cli.verbose,
            &cli.features,
            cli.no_default_features,
            cli.target.clone(),
            cli.untrusted,
        ),
        Commands::Build { release, jobs, force, remote_cache, no_hooks } => {
            commands::build::run(release, cli.locked, cli.offline, cli.verbose, cli.target, jobs, force, remote_cache, no_hooks)
        }
        Commands::Test { release } => {
            commands::test::run(release, cli.locked, cli.offline, cli.verbose, cli.target)
        }
        Commands::Update { name, patch } => commands::update::run(name, patch, cli.verbose),
        Commands::Deps { tree } => commands::deps::run(tree),
        Commands::Cache { action } => match action {
            CacheAction::Status => commands::cache::status(),
            CacheAction::Clean => commands::cache::clean(),
            CacheAction::Push => commands::cache::push(cli.verbose),
            CacheAction::Pull => commands::cache::pull(cli.verbose),
        },
        Commands::Verify { signatures } => commands::verify::run(cli.verbose, signatures),
        Commands::Graph { format, filter } => commands::graph::run(format, filter),
        Commands::Audit => commands::audit::run(cli.verbose),
        Commands::Status => commands::status::run(cli.verbose),
        Commands::Explain { module } => commands::explain::run(module, cli.verbose),
        Commands::Toolchain { action } => match action {
            ToolchainAction::Show => commands::toolchain::show(cli.verbose),
            ToolchainAction::Check => commands::toolchain::check(),
        },
        Commands::Vendor { sync } => commands::vendor::run(sync, cli.verbose),
        Commands::Lint => commands::lint::run(cli.verbose),
        Commands::Fmt { check } => commands::fmt::run(check, cli.verbose),
        Commands::Search { query } => commands::search::run(&query, cli.verbose),
        Commands::Run { release, args } => commands::run::run(release, args, cli.verbose),
        Commands::Clean => commands::clean::run(cli.verbose),
        Commands::Workspace { action } => match action {
            WorkspaceAction::List => commands::workspace::list(cli.verbose),
            WorkspaceAction::Add { name } => commands::workspace::add(&name, cli.verbose),
            WorkspaceAction::Remove { name } => commands::workspace::remove(&name, cli.verbose),
        },
        Commands::Sbom { output } => commands::sbom::run(output, cli.verbose),
        Commands::Publish { dry_run, push } => commands::publish::run(dry_run, push, cli.verbose),
        Commands::CompileCommands => commands::compile_commands::run(cli.verbose, cli.target.clone()),
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);

        // Print helpful hints based on error type
        if let Some(hint) = error_hint(&e) {
            eprintln!("  hint: {}", hint);
        }

        std::process::exit(e.exit_code());
    }
}

/// Return a helpful hint string for common errors.
fn error_hint(e: &cmod_core::error::CmodError) -> Option<&'static str> {
    use cmod_core::error::CmodError;
    match e {
        CmodError::ManifestNotFound { .. } => {
            Some("run `cmod init` to create a new project")
        }
        CmodError::LockfileNotFound => {
            Some("run `cmod resolve` to generate the lockfile")
        }
        CmodError::LockfileOutdated => {
            Some("run `cmod resolve` to update the lockfile")
        }
        CmodError::DependencyNotFound { .. } => {
            Some("check the dependency name or add it with `cmod add <dep>`")
        }
        CmodError::CompilerNotFound { .. } => {
            Some("ensure clang is installed and available on PATH")
        }
        CmodError::GitRepoNotFound { .. } => {
            Some("check the Git URL and your network connection")
        }
        CmodError::CircularDependency { .. } => {
            Some("review your dependency graph with `cmod deps --tree`")
        }
        _ => None,
    }
}
