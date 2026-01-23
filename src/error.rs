use thiserror::Error;

/// Custom error types for the rqcheck application
#[derive(Error, Debug)]
pub enum RqCheckError {
    #[error("Failed to read requirements file '{path}': {source}")]
    FileReadError {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to parse requirements file: {0}")]
    RequirementsParseError(String),

    #[error("Failed to load configuration: {0}")]
    ConfigError(String),

    #[error("Network error while checking package '{package}': {source}")]
    NetworkError {
        package: String,
        source: reqwest::Error,
    },

    #[error("Failed to parse version for package '{package}': {reason}")]
    VersionParseError { package: String, reason: String },

    #[error("Invalid package requirement: {0}")]
    InvalidRequirement(String),

    #[error("HTTP request failed for package '{package}': status {status}")]
    HttpError { package: String, status: u16 },

    #[error("Timeout while checking package '{package}'")]
    TimeoutError { package: String },
}

pub type Result<T> = std::result::Result<T, RqCheckError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_read_error_display() {
        let error = RqCheckError::FileReadError {
            path: "test.txt".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        let msg = format!("{}", error);
        assert!(msg.contains("test.txt"));
        assert!(msg.contains("Failed to read requirements file"));
    }

    #[test]
    fn test_requirements_parse_error_display() {
        let error = RqCheckError::RequirementsParseError("invalid syntax".to_string());
        let msg = format!("{}", error);
        assert!(msg.contains("Failed to parse requirements file"));
        assert!(msg.contains("invalid syntax"));
    }

    #[test]
    fn test_config_error_display() {
        let error = RqCheckError::ConfigError("invalid config".to_string());
        let msg = format!("{}", error);
        assert!(msg.contains("Failed to load configuration"));
        assert!(msg.contains("invalid config"));
    }

    #[test]
    fn test_version_parse_error_display() {
        let error = RqCheckError::VersionParseError {
            package: "requests".to_string(),
            reason: "empty version".to_string(),
        };
        let msg = format!("{}", error);
        assert!(msg.contains("requests"));
        assert!(msg.contains("empty version"));
    }

    #[test]
    fn test_invalid_requirement_display() {
        let error = RqCheckError::InvalidRequirement("missing name".to_string());
        let msg = format!("{}", error);
        assert!(msg.contains("Invalid package requirement"));
        assert!(msg.contains("missing name"));
    }

    #[test]
    fn test_http_error_display() {
        let error = RqCheckError::HttpError {
            package: "flask".to_string(),
            status: 404,
        };
        let msg = format!("{}", error);
        assert!(msg.contains("flask"));
        assert!(msg.contains("404"));
    }
}

