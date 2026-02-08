pub mod checker;
pub mod config;
pub mod error;
pub mod version;

use checker::{check_packages_batched, PackageCheckResult, PackageRequirement};
use clap::Parser;
use config::Config;
use error::{Result, RqCheckError};
use std::fs;

/// Simple check for new versions of python packages (pypi.org)
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Requirements file name
    #[arg(short, long)]
    file_name: String,

    /// Configuration file path (optional)
    #[arg(short, long)]
    config: Option<String>,

    /// Batch size for concurrent requests
    #[arg(short, long)]
    batch_size: Option<usize>,

    /// Timeout in seconds for HTTP requests
    #[arg(short, long)]
    timeout: Option<u64>,

    /// Maximum number of versions to check (default: 10)
    #[arg(short = 'n', long)]
    max_versions: Option<usize>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    init_logging(args.verbose);

    // Load configuration
    let mut config = Config::load(args.config.as_deref())?;
    config.apply_cli_overrides(args.batch_size, args.timeout, args.max_versions, args.verbose);

    if config.verbose {
        log::info!("Configuration: {:?}", config);
    }

    // Read and parse requirements file
    let requirements = read_requirements(&args.file_name)?;

    if requirements.is_empty() {
        log::warn!("No valid requirements found in file");
        return Ok(());
    }

    log::info!("Found {} requirements to check", requirements.len());

    // Convert to owned data for async processing
    let package_reqs = convert_requirements(requirements)?;

    // Check packages in batches
    let results = check_packages_batched(package_reqs, &config).await;

    // Display results
    display_results(&results);

    Ok(())
}

/// Initialize logging based on verbosity level
fn init_logging(verbose: bool) {
    let log_level = if verbose { "debug" } else { "info" };
    
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_timestamp(None)
        .format_module_path(false)
        .init();
}

/// Read and parse requirements file
fn read_requirements(file_path: &str) -> Result<Vec<requirements::Requirement<'static>>> {
    let contents = fs::read_to_string(file_path).map_err(|e| RqCheckError::FileReadError {
        path: file_path.to_string(),
        source: e,
    })?;

    // Parse requirements - we need to leak the string to get 'static lifetime
    // This is acceptable since we only do this once at startup
    let static_contents = Box::leak(contents.into_boxed_str());
    
    let reqs = requirements::parse_str(static_contents).map_err(|e| {
        RqCheckError::RequirementsParseError(format!("Failed to parse requirements: {:?}", e))
    })?;

    // Filter out requirements without names (comments, blank lines, etc.)
    let valid_reqs: Vec<_> = reqs.into_iter().filter(|req| req.name.is_some()).collect();

    Ok(valid_reqs)
}

/// Convert requirements to owned data structure
fn convert_requirements(requirements: Vec<requirements::Requirement>) -> Result<Vec<PackageRequirement>> {
    let mut package_reqs = Vec::new();

    for req in requirements {
        let name = req
            .name
            .ok_or_else(|| RqCheckError::InvalidRequirement("Missing package name".to_string()))?
            .to_string();

        // Extract version with error handling
        if req.specs.is_empty() {
            return Err(RqCheckError::VersionParseError {
                package: name.clone(),
                reason: "No version specified in requirements".to_string(),
            });
        }

        let mut version = req.specs[0].1.clone();
        
        // Remove leading 'v' if present
        if version.starts_with('v') || version.starts_with('V') {
            version.remove(0);
        }

        if version.is_empty() {
            return Err(RqCheckError::VersionParseError {
                package: name.clone(),
                reason: "Empty version string".to_string(),
            });
        }

        package_reqs.push(PackageRequirement { name, version });
    }

    Ok(package_reqs)
}

/// Display check results
fn display_results(results: &[PackageCheckResult]) {
    let updates: Vec<_> = results.iter().filter(|r| r.has_update).collect();

    if updates.is_empty() {
        log::info!("All packages are up to date!");
    } else {
        log::info!("Found {} package(s) with updates:", updates.len());
        for result in updates {
            let mut output = format!(
                "{} {} -> latest: {}",
                result.name, result.current_version, result.latest_version
            );

            // Add latest major version if different from latest
            if let Some(ref latest_major) = result.latest_major_version {
                output.push_str(&format!(", latest major: {}", latest_major));
            }

            // Add latest minor version if available
            if let Some(ref latest_minor) = result.latest_minor_version {
                output.push_str(&format!(", latest minor: {}", latest_minor));
            }

            println!("{}", output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_parse_valid_requirements() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "requests==2.28.0").unwrap();
        writeln!(temp_file, "flask==2.3.0").unwrap();
        writeln!(temp_file, "django==4.2.0").unwrap();

        let result = read_requirements(temp_file.path().to_str().unwrap());
        assert!(result.is_ok());
        
        let reqs = result.unwrap();
        assert_eq!(reqs.len(), 3);
    }

    #[test]
    fn test_parse_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        
        let result = read_requirements(temp_file.path().to_str().unwrap());
        assert!(result.is_ok());
        
        let reqs = result.unwrap();
        assert_eq!(reqs.len(), 0);
    }

    #[test]
    fn test_parse_comments_only() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "# This is a comment").unwrap();
        writeln!(temp_file, "# Another comment").unwrap();
        writeln!(temp_file, "").unwrap();

        let result = read_requirements(temp_file.path().to_str().unwrap());
        assert!(result.is_ok());
        
        let reqs = result.unwrap();
        assert_eq!(reqs.len(), 0);
    }

    #[test]
    fn test_convert_requirements() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "requests==2.28.0").unwrap();
        writeln!(temp_file, "flask==v2.3.0").unwrap(); // Test 'v' prefix removal

        let reqs = read_requirements(temp_file.path().to_str().unwrap()).unwrap();
        let result = convert_requirements(reqs);
        
        assert!(result.is_ok());
        let package_reqs = result.unwrap();
        assert_eq!(package_reqs.len(), 2);
        assert_eq!(package_reqs[0].name, "requests");
        assert_eq!(package_reqs[0].version, "2.28.0");
        assert_eq!(package_reqs[1].name, "flask");
        assert_eq!(package_reqs[1].version, "2.3.0"); // 'v' removed
    }

    #[test]
    fn test_file_not_found() {
        let result = read_requirements("/nonexistent/requirements.txt");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RqCheckError::FileReadError { .. }));
    }

    #[test]
    fn test_mixed_content() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "# Comment").unwrap();
        writeln!(temp_file, "requests==2.28.0").unwrap();
        writeln!(temp_file, "").unwrap();
        writeln!(temp_file, "flask==2.3.0").unwrap();
        writeln!(temp_file, "# Another comment").unwrap();

        let result = read_requirements(temp_file.path().to_str().unwrap());
        assert!(result.is_ok());
        
        let reqs = result.unwrap();
        assert_eq!(reqs.len(), 2); // Only actual packages
    }

    #[test]
    fn test_version_with_uppercase_v() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "package==V1.2.3").unwrap();

        let reqs = read_requirements(temp_file.path().to_str().unwrap()).unwrap();
        let result = convert_requirements(reqs);
        
        assert!(result.is_ok());
        let package_reqs = result.unwrap();
        assert_eq!(package_reqs[0].version, "1.2.3"); // 'V' removed
    }

    #[test]
    fn test_display_results_with_updates() {
        let results = vec![
            PackageCheckResult {
                name: "requests".to_string(),
                current_version: "2.28.0".to_string(),
                latest_version: "2.32.0".to_string(),
                latest_major_version: None,
                latest_minor_version: Some("2.32.0".to_string()),
                has_update: true,
            },
            PackageCheckResult {
                name: "flask".to_string(),
                current_version: "2.3.0".to_string(),
                latest_version: "2.3.0".to_string(),
                latest_major_version: None,
                latest_minor_version: None,
                has_update: false,
            },
        ];

        // This test just ensures display_results doesn't panic
        display_results(&results);
    }

    #[test]
    fn test_display_results_no_updates() {
        let results = vec![
            PackageCheckResult {
                name: "requests".to_string(),
                current_version: "2.28.0".to_string(),
                latest_version: "2.28.0".to_string(),
                latest_major_version: None,
                latest_minor_version: None,
                has_update: false,
            },
        ];

        // This test just ensures display_results doesn't panic
        display_results(&results);
    }
}

