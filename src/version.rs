use semver::Version;
use std::collections::HashMap;
use log::{debug, warn};

#[derive(Debug, Clone, PartialEq)]
pub struct VersionAnalysis {
    pub latest: String,
    pub latest_major: Option<String>,
    pub latest_minor: Option<String>,
    pub has_update: bool,
}

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub yanked: bool,
}

/// Analyze available versions and provide upgrade recommendations
///
/// # Arguments
/// * `current_version` - The current version string
/// * `releases` - HashMap of version strings to release info
/// * `max_versions` - Maximum number of versions to consider
///
/// # Returns
/// VersionAnalysis with recommendations for latest, latest_major, and latest_minor
pub fn analyze_versions(
    current_version: &str,
    releases: &HashMap<String, Vec<ReleaseInfo>>,
    max_versions: usize,
) -> VersionAnalysis {
    debug!("Analyzing versions for current: {}", current_version);

    // Parse current version
    let current = match Version::parse(current_version) {
        Ok(v) => v,
        Err(e) => {
            warn!("Failed to parse current version '{}': {}", current_version, e);
            return VersionAnalysis {
                latest: current_version.to_string(),
                latest_major: None,
                latest_minor: None,
                has_update: false,
            };
        }
    };

    // Filter and sort versions
    let mut valid_versions: Vec<Version> = releases
        .iter()
        .filter_map(|(version_str, release_infos)| {
            // Skip if any release info indicates yanked
            if release_infos.iter().any(|info| info.yanked) {
                debug!("Skipping yanked version: {}", version_str);
                return None;
            }

            // Try to parse as semver
            match Version::parse(version_str) {
                Ok(v) => {
                    // Skip pre-release versions
                    if !v.pre.is_empty() {
                        debug!("Skipping pre-release version: {}", version_str);
                        return None;
                    }
                    Some(v)
                }
                Err(e) => {
                    debug!("Skipping invalid semver '{}': {}", version_str, e);
                    None
                }
            }
        })
        .collect();

    // Sort in descending order (newest first)
    valid_versions.sort_by(|a, b| b.cmp(a));

    // Limit to max_versions
    valid_versions.truncate(max_versions);

    debug!("Valid versions after filtering: {:?}", valid_versions);

    if valid_versions.is_empty() {
        warn!("No valid versions found in releases");
        return VersionAnalysis {
            latest: current_version.to_string(),
            latest_major: None,
            latest_minor: None,
            has_update: false,
        };
    }

    // Latest version overall
    let latest = valid_versions[0].clone();

    // Check if there's an update available
    let has_update = latest > current;

    // Find latest major version (different from latest)
    let latest_major = if has_update {
        valid_versions
            .iter()
            .skip(1) // Skip the overall latest
            .find(|v| v.major != latest.major)
            .map(|v| v.to_string())
    } else {
        None
    };

    // Find latest minor version in current major line
    let latest_minor = if has_update {
        valid_versions
            .iter()
            .find(|v| v.major == current.major && **v > current)
            .map(|v| v.to_string())
    } else {
        None
    };

    debug!(
        "Analysis complete - latest: {}, latest_major: {:?}, latest_minor: {:?}, has_update: {}",
        latest, latest_major, latest_minor, has_update
    );

    VersionAnalysis {
        latest: latest.to_string(),
        latest_major,
        latest_minor,
        has_update,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_releases(versions: &[&str]) -> HashMap<String, Vec<ReleaseInfo>> {
        versions
            .iter()
            .map(|v| (v.to_string(), vec![ReleaseInfo { yanked: false }]))
            .collect()
    }

    fn create_releases_with_yanked(versions: &[(&str, bool)]) -> HashMap<String, Vec<ReleaseInfo>> {
        versions
            .iter()
            .map(|(v, yanked)| (v.to_string(), vec![ReleaseInfo { yanked: *yanked }]))
            .collect()
    }

    #[test]
    fn test_major_update_available() {
        let releases = create_releases(&["3.0.0", "2.5.0", "2.0.5", "1.9.0"]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "3.0.0");
        assert_eq!(analysis.latest_major, Some("2.5.0".to_string()));
        assert_eq!(analysis.latest_minor, Some("2.5.0".to_string())); // Latest in current major (2.x)
        assert!(analysis.has_update);
    }

    #[test]
    fn test_minor_update_available() {
        let releases = create_releases(&["2.5.0", "2.3.0", "2.0.5", "1.9.0"]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "2.5.0");
        assert_eq!(analysis.latest_major, Some("1.9.0".to_string()));
        assert_eq!(analysis.latest_minor, Some("2.5.0".to_string()));
        assert!(analysis.has_update);
    }

    #[test]
    fn test_patch_update_available() {
        let releases = create_releases(&["2.0.5", "2.0.3", "2.0.1", "1.9.0"]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "2.0.5");
        assert_eq!(analysis.latest_major, Some("1.9.0".to_string()));
        assert_eq!(analysis.latest_minor, Some("2.0.5".to_string()));
        assert!(analysis.has_update);
    }

    #[test]
    fn test_already_on_latest() {
        let releases = create_releases(&["2.0.0", "1.9.0", "1.8.0"]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "2.0.0");
        assert_eq!(analysis.latest_major, None);
        assert_eq!(analysis.latest_minor, None);
        assert!(!analysis.has_update);
    }

    #[test]
    fn test_skip_prerelease_versions() {
        let releases = create_releases(&["3.0.0-rc1", "3.0.0-beta", "2.5.0", "2.0.0"]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "2.5.0");
        assert!(analysis.has_update);
    }

    #[test]
    fn test_skip_yanked_versions() {
        let releases = create_releases_with_yanked(&[
            ("3.0.0", true),
            ("2.5.0", false),
            ("2.0.0", false),
        ]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "2.5.0");
        assert!(analysis.has_update);
    }

    #[test]
    fn test_invalid_semver_versions() {
        let mut releases = create_releases(&["2.5.0", "2.0.0"]);
        releases.insert("invalid-version".to_string(), vec![ReleaseInfo { yanked: false }]);
        releases.insert("2024.01.01".to_string(), vec![ReleaseInfo { yanked: false }]);

        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "2.5.0");
        assert!(analysis.has_update);
    }

    #[test]
    fn test_version_limit() {
        let releases = create_releases(&[
            "5.0.0", "4.0.0", "3.0.0", "2.5.0", "2.4.0",
            "2.3.0", "2.2.0", "2.1.0", "2.0.5", "2.0.0",
            "1.9.0", "1.8.0", "1.7.0",
        ]);
        let analysis = analyze_versions("2.0.0", &releases, 5);

        // Should only consider top 5 versions: 5.0.0, 4.0.0, 3.0.0, 2.5.0, 2.4.0
        assert_eq!(analysis.latest, "5.0.0");
        assert_eq!(analysis.latest_major, Some("4.0.0".to_string()));
        assert_eq!(analysis.latest_minor, Some("2.5.0".to_string()));
    }

    #[test]
    fn test_fewer_versions_than_limit() {
        let releases = create_releases(&["2.5.0", "2.0.0"]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "2.5.0");
        assert!(analysis.has_update);
    }

    #[test]
    fn test_invalid_current_version() {
        let releases = create_releases(&["2.5.0", "2.0.0"]);
        let analysis = analyze_versions("invalid", &releases, 10);

        assert_eq!(analysis.latest, "invalid");
        assert_eq!(analysis.latest_major, None);
        assert_eq!(analysis.latest_minor, None);
        assert!(!analysis.has_update);
    }

    #[test]
    fn test_no_valid_releases() {
        let releases = create_releases(&[]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "2.0.0");
        assert!(!analysis.has_update);
    }

    #[test]
    fn test_only_older_versions() {
        let releases = create_releases(&["1.9.0", "1.8.0", "1.7.0"]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "1.9.0");
        assert!(!analysis.has_update);
    }

    #[test]
    fn test_latest_minor_same_as_latest() {
        // When latest is in the same major line as current
        let releases = create_releases(&["2.5.0", "2.3.0", "1.9.0"]);
        let analysis = analyze_versions("2.0.0", &releases, 10);

        assert_eq!(analysis.latest, "2.5.0");
        assert_eq!(analysis.latest_major, Some("1.9.0".to_string()));
        assert_eq!(analysis.latest_minor, Some("2.5.0".to_string()));
    }
}
