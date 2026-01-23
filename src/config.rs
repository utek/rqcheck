use crate::error::{Result, RqCheckError};
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Application configuration with support for file, environment, and CLI overrides
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,

    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,

    #[serde(default = "default_pypi_base_url")]
    pub pypi_base_url: String,

    #[serde(default)]
    pub verbose: bool,
}

// Default value functions
fn default_batch_size() -> usize {
    10
}

fn default_timeout_seconds() -> u64 {
    30
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay_ms() -> u64 {
    1000
}

fn default_pypi_base_url() -> String {
    "https://pypi.org/pypi".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            batch_size: default_batch_size(),
            timeout_seconds: default_timeout_seconds(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            pypi_base_url: default_pypi_base_url(),
            verbose: false,
        }
    }
}

impl Config {
    /// Load configuration from file, environment variables, and CLI arguments
    pub fn load(config_path: Option<&str>) -> Result<Self> {
        let mut config = Self::default();

        // Load from config file if provided
        if let Some(path) = config_path {
            if Path::new(path).exists() {
                config = Self::from_file(path)?;
                if config.verbose {
                    log::info!("Loaded configuration from: {}", path);
                }
            } else {
                log::warn!("Config file not found: {}, using defaults", path);
            }
        }

        // Override with environment variables
        config.apply_env_overrides();

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Load configuration from a TOML file
    fn from_file(path: &str) -> Result<Self> {
        let contents = fs::read_to_string(path).map_err(|e| RqCheckError::ConfigError {
            0: format!("Failed to read config file '{}': {}", path, e),
        })?;

        toml::from_str(&contents).map_err(|e| RqCheckError::ConfigError {
            0: format!("Failed to parse config file '{}': {}", path, e),
        })
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("RQCHECK_BATCH_SIZE") {
            if let Ok(size) = val.parse() {
                self.batch_size = size;
                if self.verbose {
                    log::info!("Batch size overridden by environment: {}", size);
                }
            }
        }

        if let Ok(val) = std::env::var("RQCHECK_TIMEOUT") {
            if let Ok(timeout) = val.parse() {
                self.timeout_seconds = timeout;
            }
        }

        if let Ok(val) = std::env::var("RQCHECK_MAX_RETRIES") {
            if let Ok(retries) = val.parse() {
                self.max_retries = retries;
            }
        }

        if let Ok(val) = std::env::var("RQCHECK_VERBOSE") {
            self.verbose = val.to_lowercase() == "true" || val == "1";
        }

        if let Ok(url) = std::env::var("RQCHECK_PYPI_URL") {
            self.pypi_base_url = url;
        }
    }

    /// Validate configuration values
    fn validate(&self) -> Result<()> {
        if self.batch_size == 0 {
            return Err(RqCheckError::ConfigError(
                "batch_size must be greater than 0".to_string(),
            ));
        }

        if self.batch_size > 100 {
            log::warn!(
                "Large batch size ({}) may overwhelm PyPI servers",
                self.batch_size
            );
        }

        if self.timeout_seconds == 0 {
            return Err(RqCheckError::ConfigError(
                "timeout_seconds must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Apply CLI argument overrides
    pub fn apply_cli_overrides(
        &mut self,
        batch_size: Option<usize>,
        timeout: Option<u64>,
        verbose: bool,
    ) {
        if let Some(size) = batch_size {
            self.batch_size = size;
        }

        if let Some(t) = timeout {
            self.timeout_seconds = t;
        }

        if verbose {
            self.verbose = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::NamedTempFile;
    use std::io::Write;
    use serial_test::serial;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.timeout_seconds, 30);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert_eq!(config.pypi_base_url, "https://pypi.org/pypi");
        assert_eq!(config.verbose, false);
    }

    #[test]
    #[serial]
    fn test_load_valid_toml() {
        // Cleanup env vars to avoid pollution from other tests
        env::remove_var("RQCHECK_BATCH_SIZE");
        env::remove_var("RQCHECK_TIMEOUT");
        env::remove_var("RQCHECK_MAX_RETRIES");
        env::remove_var("RQCHECK_VERBOSE");
        env::remove_var("RQCHECK_PYPI_URL");
        
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            r#"
batch_size = 20
timeout_seconds = 60
max_retries = 5
retry_delay_ms = 2000
pypi_base_url = "https://test.pypi.org"
verbose = true
"#
        )
        .unwrap();

        let config = Config::load(Some(temp_file.path().to_str().unwrap())).unwrap();
        assert_eq!(config.batch_size, 20);
        assert_eq!(config.timeout_seconds, 60);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_delay_ms, 2000);
        assert_eq!(config.pypi_base_url, "https://test.pypi.org");
        assert_eq!(config.verbose, true);
    }

    #[test]
    fn test_load_invalid_toml() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "invalid toml content {{").unwrap();

        let result = Config::load(Some(temp_file.path().to_str().unwrap()));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RqCheckError::ConfigError(_)));
    }

    #[test]
    fn test_missing_config_file() {
        // Should use defaults when file doesn't exist
        let config = Config::load(Some("/nonexistent/path/config.toml")).unwrap();
        assert_eq!(config.batch_size, 10); // Default value
    }

    #[test]
    #[serial]
    fn test_env_override() {
        // Cleanup first
        env::remove_var("RQCHECK_BATCH_SIZE");
        env::remove_var("RQCHECK_TIMEOUT");
        env::remove_var("RQCHECK_MAX_RETRIES");
        env::remove_var("RQCHECK_VERBOSE");
        env::remove_var("RQCHECK_PYPI_URL");
        
        env::set_var("RQCHECK_BATCH_SIZE", "15");
        env::set_var("RQCHECK_TIMEOUT", "45");
        env::set_var("RQCHECK_MAX_RETRIES", "7");
        env::set_var("RQCHECK_VERBOSE", "true");
        env::set_var("RQCHECK_PYPI_URL", "https://custom.pypi.org");

        let config = Config::load(None).unwrap();
        assert_eq!(config.batch_size, 15);
        assert_eq!(config.timeout_seconds, 45);
        assert_eq!(config.max_retries, 7);
        assert_eq!(config.verbose, true);
        assert_eq!(config.pypi_base_url, "https://custom.pypi.org");

        // Cleanup
        env::remove_var("RQCHECK_BATCH_SIZE");
        env::remove_var("RQCHECK_TIMEOUT");
        env::remove_var("RQCHECK_MAX_RETRIES");
        env::remove_var("RQCHECK_VERBOSE");
        env::remove_var("RQCHECK_PYPI_URL");
    }

    #[test]
    fn test_cli_override() {
        let mut config = Config::default();
        config.apply_cli_overrides(Some(25), Some(90), true);

        assert_eq!(config.batch_size, 25);
        assert_eq!(config.timeout_seconds, 90);
        assert_eq!(config.verbose, true);
    }

    #[test]
    fn test_validate_zero_batch_size() {
        let mut config = Config::default();
        config.batch_size = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RqCheckError::ConfigError(_)));
    }

    #[test]
    fn test_validate_zero_timeout() {
        let mut config = Config::default();
        config.timeout_seconds = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RqCheckError::ConfigError(_)));
    }

    #[test]
    #[serial]
    fn test_override_precedence() {
        // Cleanup first in case previous test left env var
        env::remove_var("RQCHECK_BATCH_SIZE");
        
        // Create config file with batch_size = 20
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "batch_size = 20").unwrap();

        // Load config (should have file value since no env var)
        let config = Config::load(Some(temp_file.path().to_str().unwrap())).unwrap();
        assert_eq!(config.batch_size, 20); // File value

        // Now set env variable to 30
        env::set_var("RQCHECK_BATCH_SIZE", "30");

        // Load config again (should have env value)
        let mut config = Config::load(Some(temp_file.path().to_str().unwrap())).unwrap();
        assert_eq!(config.batch_size, 30); // Env overrides file

        // Apply CLI override to 40
        config.apply_cli_overrides(Some(40), None, false);
        assert_eq!(config.batch_size, 40); // CLI overrides env

        // Cleanup
        env::remove_var("RQCHECK_BATCH_SIZE");
    }

    #[test]
    fn test_partial_toml() {
        // Test that missing fields use defaults
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "batch_size = 15").unwrap();

        let config = Config::load(Some(temp_file.path().to_str().unwrap())).unwrap();
        assert_eq!(config.batch_size, 15);
        assert_eq!(config.timeout_seconds, 30); // Default
        assert_eq!(config.max_retries, 3); // Default
    }
}

