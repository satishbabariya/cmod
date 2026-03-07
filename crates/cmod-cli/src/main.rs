mod commands;

use clap::{Parser, Subcommand};
use cmod_core::shell::{Shell, Verbosity};

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
    #[arg(long, short, global = true, conflicts_with = "quiet")]
    verbose: bool,

    /// Suppress all status output
    #[arg(long, short, global = true, conflicts_with = "verbose")]
    quiet: bool,

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

impl Cli {
    fn verbosity(&self) -> Verbosity {
        if self.quiet {
            Verbosity::Quiet
        } else if self.verbose {
            Verbosity::Verbose
        } else {
            Verbosity::Normal
        }
    }
}

#[derive(Subcommand)]
#[allow(clippy::enum_variant_names)]
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

        /// Verify lockfile integrity and package hashes before building
        #[arg(long)]
        verify: bool,

        /// Display per-module compile timings
        #[arg(long)]
        timings: bool,

        /// Enable distributed build across remote workers
        #[arg(long)]
        distributed: bool,

        /// Worker endpoints for distributed builds (comma-separated URLs)
        #[arg(long, value_delimiter = ',')]
        workers: Vec<String>,
    },

    /// Run module tests
    Test {
        /// Build in release mode
        #[arg(long)]
        release: bool,

        /// Test name filter (positional, optional)
        #[arg(value_name = "TESTNAME")]
        name: Option<String>,

        /// Filter tests by glob pattern
        #[arg(long, short = 'f')]
        filter: Option<String>,

        /// Maximum parallel test execution jobs (0 = auto)
        #[arg(long, short, default_value = "0")]
        jobs: usize,

        /// Continue running after test failures
        #[arg(long)]
        no_fail_fast: bool,

        /// Per-test timeout in seconds (0 = no timeout)
        #[arg(long, default_value = "0")]
        timeout: u64,

        /// Test a specific workspace member
        #[arg(short, long)]
        package: Option<String>,

        /// Enable LLVM source-based code coverage
        #[arg(long)]
        coverage: bool,

        /// Enable sanitizers (comma-separated: address, undefined, thread, memory)
        #[arg(long, value_delimiter = ',')]
        sanitize: Vec<String>,

        /// Output format for test results (human, json, junit, tap)
        #[arg(long, value_name = "FORMAT")]
        format: Option<String>,
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

        /// Explain why a specific dependency is included
        #[arg(long)]
        why: Option<String>,

        /// Show transitive dependency conflicts
        #[arg(long)]
        conflicts: bool,
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

        /// Show build status annotations (up-to-date, needs-rebuild, never-built)
        #[arg(long)]
        status: bool,

        /// Highlight the critical path (longest compile chain)
        #[arg(long)]
        critical_path: bool,

        /// Annotate nodes with build timing (color-coded by duration)
        #[arg(long)]
        timing: bool,
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
    Lint {
        /// Treat warnings as errors (non-zero exit code if any warnings)
        #[arg(long)]
        deny_warnings: bool,

        /// Lint a specific workspace member
        #[arg(short, long)]
        package: Option<String>,
    },

    /// Format C++ source files using clang-format
    Fmt {
        /// Check formatting without modifying files
        #[arg(long)]
        check: bool,

        /// Format a specific workspace member
        #[arg(short, long)]
        package: Option<String>,
    },

    /// Search for modules by name
    Search {
        /// Search query (substring match)
        query: String,

        /// Only search local dependencies and lockfile
        #[arg(long)]
        local_only: bool,
    },

    /// Build and run the project binary
    Run {
        /// Build in release mode
        #[arg(long)]
        release: bool,

        /// Run a specific workspace member
        #[arg(short, long)]
        package: Option<String>,

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

        /// Sign the release tag
        #[arg(long)]
        sign: bool,

        /// Do not sign the release tag (overrides [security] config)
        #[arg(long, conflicts_with = "sign")]
        no_sign: bool,

        /// Skip governance policy validation
        #[arg(long)]
        skip_governance: bool,
    },

    /// Generate compile_commands.json for IDE integration
    CompileCommands,

    /// Remove unused dependencies from cmod.toml
    Tidy {
        /// Actually remove unused deps (default: dry run)
        #[arg(long)]
        apply: bool,
    },

    /// Validate module naming, identity, and structure rules
    Check,

    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Output the build plan as JSON without executing it
    Plan,

    /// Export a CMakeLists.txt for interop with CMake-based projects
    EmitCmake,

    /// Start the LSP server for IDE integration
    Lsp,
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
enum PluginAction {
    /// List discovered plugins
    List,
    /// Run a plugin by name
    Run {
        /// Plugin name
        name: String,
    },
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
    /// Run garbage collection (evict old/oversized entries)
    Gc,
    /// Export a cached module as a BMI package
    Export {
        /// Module name to export
        module: String,
        /// Cache key of the entry to export
        key: String,
        /// Output directory
        #[arg(short, long)]
        output: String,
    },
    /// Import a BMI package into the local cache
    Import {
        /// Path to the BMI package directory
        path: String,
    },
    /// Show cache status as JSON (machine-readable)
    StatusJson,
    /// Inspect a specific cache entry
    Inspect {
        /// Module name
        module: String,
        /// Cache key (hex)
        key: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let shell = Shell::new(cli.verbosity());

    let result = match cli.command {
        Commands::Init { workspace, name } => commands::init::run(workspace, name, &shell),
        Commands::Add {
            dep,
            git,
            branch,
            rev,
            path,
            features,
        } => commands::add::run(
            dep,
            git,
            branch,
            rev,
            path,
            features,
            cli.locked,
            cli.offline,
            &shell,
        ),
        Commands::Remove { name } => commands::remove::run(name, &shell),
        Commands::Resolve => commands::resolve::run(
            cli.locked,
            cli.offline,
            &shell,
            &cli.features,
            cli.no_default_features,
            cli.target.clone(),
            cli.untrusted,
        ),
        Commands::Build {
            release,
            jobs,
            force,
            remote_cache,
            no_hooks,
            verify,
            timings,
            distributed,
            workers,
        } => commands::build::run(
            release,
            cli.locked,
            cli.offline,
            &shell,
            cli.target,
            jobs,
            force,
            remote_cache,
            no_hooks,
            verify,
            timings,
            &cli.features,
            cli.no_default_features,
            cli.no_cache,
            distributed,
            workers,
        ),
        Commands::Test {
            release,
            name,
            filter,
            jobs,
            no_fail_fast,
            timeout,
            package,
            coverage,
            sanitize,
            format,
        } => commands::test::run(
            release,
            cli.locked,
            cli.offline,
            &shell,
            cli.target,
            cli.no_cache,
            name,
            filter,
            jobs,
            no_fail_fast,
            timeout,
            package,
            coverage,
            sanitize,
            format,
        ),
        Commands::Update { name, patch } => commands::update::run(name, patch, &shell),
        Commands::Deps {
            tree,
            why,
            conflicts,
        } => commands::deps::run(tree, why, conflicts, &shell),
        Commands::Cache { action } => match action {
            CacheAction::Status => commands::cache::status(&shell),
            CacheAction::Clean => commands::cache::clean(&shell),
            CacheAction::Push => commands::cache::push(&shell),
            CacheAction::Pull => commands::cache::pull(&shell),
            CacheAction::Gc => commands::cache::gc(&shell),
            CacheAction::Export {
                module,
                key,
                output,
            } => commands::cache::export_bmi(&module, &key, &output, &shell),
            CacheAction::Import { path } => commands::cache::import_bmi(&path, &shell),
            CacheAction::StatusJson => commands::cache::status_json(),
            CacheAction::Inspect { module, key } => commands::cache::inspect(&module, &key),
        },
        Commands::Verify { signatures } => commands::verify::run(&shell, signatures),
        Commands::Graph {
            format,
            filter,
            status,
            critical_path,
            timing,
        } => commands::graph::run(format, filter, status, critical_path, timing, &shell),
        Commands::Audit => commands::audit::run(&shell),
        Commands::Status => commands::status::run(&shell),
        Commands::Explain { module } => commands::explain::run(module, &shell),
        Commands::Toolchain { action } => match action {
            ToolchainAction::Show => commands::toolchain::show(&shell),
            ToolchainAction::Check => commands::toolchain::check(&shell),
        },
        Commands::Vendor { sync } => commands::vendor::run(sync, &shell),
        Commands::Lint {
            deny_warnings,
            package,
        } => commands::lint::run(deny_warnings, package, &shell),
        Commands::Fmt { check, package } => commands::fmt::run(check, package, &shell),
        Commands::Search { query, local_only } => {
            commands::search::run(&query, local_only, cli.offline, &shell)
        }
        Commands::Run {
            release,
            package,
            args,
        } => commands::run::run(release, package, args, &shell, cli.no_cache),
        Commands::Clean => commands::clean::run(&shell),
        Commands::Workspace { action } => match action {
            WorkspaceAction::List => commands::workspace::list(&shell),
            WorkspaceAction::Add { name } => commands::workspace::add(&name, &shell),
            WorkspaceAction::Remove { name } => commands::workspace::remove(&name, &shell),
        },
        Commands::Sbom { output } => commands::sbom::run(output, &shell),
        Commands::Publish {
            dry_run,
            push,
            sign,
            no_sign,
            skip_governance,
        } => commands::publish::run(dry_run, push, sign, no_sign, skip_governance, &shell),
        Commands::CompileCommands => commands::compile_commands::run(&shell, cli.target.clone()),
        Commands::Tidy { apply } => commands::tidy::run(apply, &shell),
        Commands::Check => commands::check::run(&shell),
        Commands::Plugin { action } => match action {
            PluginAction::List => commands::plugin::list(&shell),
            PluginAction::Run { name } => commands::plugin::run_plugin(&name, &shell),
        },
        Commands::Plan => commands::build::plan(&shell, cli.target.clone()),
        Commands::EmitCmake => commands::build::emit_cmake(&shell),
        Commands::Lsp => {
            let mut server = cmod_lsp::server::LspServer::new();
            server.run()
        }
    };

    if let Err(e) = result {
        shell.error(&e);

        if let Some(hint) = error_hint(&e) {
            shell.note(hint);
        }

        std::process::exit(e.exit_code());
    }
}

/// Return a helpful hint string for common errors.
fn error_hint(e: &cmod_core::error::CmodError) -> Option<&'static str> {
    use cmod_core::error::CmodError;
    match e {
        CmodError::ManifestNotFound { .. } => Some("run `cmod init` to create a new project"),
        CmodError::LockfileNotFound => Some("run `cmod resolve` to generate the lockfile"),
        CmodError::LockfileOutdated => Some("run `cmod resolve` to update the lockfile"),
        CmodError::DependencyNotFound { .. } => {
            Some("check the dependency name or add it with `cmod add <dep>`")
        }
        CmodError::CompilerNotFound { .. } => {
            Some("ensure clang is installed and available on PATH")
        }
        CmodError::GitRepoNotFound { .. } => Some("check the Git URL and your network connection"),
        CmodError::CircularDependency { .. } => {
            Some("review your dependency graph with `cmod deps --tree`")
        }
        _ => None,
    }
}
