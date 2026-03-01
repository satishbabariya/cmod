use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_security::sbom;

/// Run `cmod sbom` — generate a Software Bill of Materials.
pub fn run(output_path: Option<String>, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    // Load the lockfile
    let lockfile = if config.lockfile_path.exists() {
        Lockfile::load(&config.lockfile_path)?
    } else {
        if verbose {
            eprintln!("  No lockfile found, generating SBOM with no dependencies");
        }
        Lockfile::new()
    };

    if verbose {
        eprintln!(
            "  Generating SBOM for {} v{} ({} packages)",
            config.manifest.package.name,
            config.manifest.package.version,
            lockfile.packages.len(),
        );
    }

    let bom = sbom::generate_sbom(&config.manifest, &lockfile)?;
    let json = sbom::sbom_to_json(&bom)?;

    match output_path {
        Some(path) => {
            std::fs::write(&path, &json)?;
            eprintln!("  SBOM written to {}", path);
        }
        None => {
            // Print to stdout
            println!("{}", json);
        }
    }

    Ok(())
}
