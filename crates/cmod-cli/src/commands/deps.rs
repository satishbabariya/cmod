use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;

/// Run `cmod deps` — display the dependency graph.
pub fn run(tree: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let lockfile = Lockfile::load(&config.lockfile_path).map_err(|_| {
        CmodError::LockfileNotFound
    })?;

    if lockfile.is_empty() {
        eprintln!("  No dependencies.");
        return Ok(());
    }

    if tree {
        print_tree(&config.manifest.package.name, &lockfile);
    } else {
        print_flat(&lockfile);
    }

    Ok(())
}

/// Print dependencies as a flat list.
fn print_flat(lockfile: &Lockfile) {
    for pkg in &lockfile.packages {
        let source = pkg.commit.as_deref().unwrap_or("local");
        println!("{} v{} ({})", pkg.name, pkg.version, source);
    }
}

/// Print dependencies as a tree.
fn print_tree(root_name: &str, lockfile: &Lockfile) {
    println!("{}", root_name);

    let total = lockfile.packages.len();
    for (i, pkg) in lockfile.packages.iter().enumerate() {
        let is_last = i == total - 1;
        let prefix = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };

        println!("{}{} v{}", prefix, pkg.name, pkg.version);

        // Print transitive deps
        for (j, dep) in pkg.deps.iter().enumerate() {
            let dep_is_last = j == pkg.deps.len() - 1;
            let dep_prefix = if dep_is_last { "└── " } else { "├── " };
            println!("{}{}{}", child_prefix, dep_prefix, dep);
        }
    }
}
