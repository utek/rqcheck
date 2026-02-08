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

/// Normalize a Python version string to a format compatible with semver.
///
/// Handles common Python versioning patterns:
/// - Two-part versions: "1.2" -> "1.2.0"
/// - Single-part versions: "3" -> "3.0.0"
/// - PEP 440 pre-release suffixes: "1.0.0rc3" -> "1.0.0-rc.3", "0.60b1" -> "0.60.0-beta.1"
fn normalize_python_version(version: &str) -> String {
    let version = version.trim();

    // PEP 440 pre-release markers, ordered longest-first to avoid partial matches
    // (e.g., "alpha" before "a", "beta" before "b")
    let pre_patterns: &[(&str, &str)] = &[
        ("alpha", "alpha"),
        ("beta", "beta"),
        ("post", "post"),
        ("dev", "dev"),
        ("rc", "rc"),
        ("b", "beta"),
        ("a", "alpha"),
    ];

    let mut base = version;
    let mut pre_suffix: Option<String> = None;

    // Find PEP 440 pre-release marker that appears after a digit, with or without
    // a dot separator (e.g., "1.0.0rc3", "1.0.0.rc3", "3.0.0.post1")
    'outer: for &(pattern, label) in pre_patterns {
        let bytes = base.as_bytes();
        let plen = pattern.len();
        if bytes.len() < plen + 1 {
            continue;
        }
        for i in 1..=bytes.len() - plen {
            if base.get(i..i + plen) != Some(pattern) {
                continue;
            }
            let remainder = &base[i + plen..];
            if !(remainder.is_empty() || remainder.chars().all(|c| c.is_ascii_digit())) {
                continue;
            }
            let num = if remainder.is_empty() { "0" } else { remainder };

            // Direct attachment: "1.0.0rc3" (digit immediately before marker)
            if bytes[i - 1].is_ascii_digit() {
                pre_suffix = Some(format!("{}.{}", label, num));
                base = &version[..i];
                break 'outer;
            }

            // Dot-separated: "3.0.0.post1" (dot + digit before marker)
            if bytes[i - 1] == b'.' && i >= 2 && bytes[i - 2].is_ascii_digit() {
                pre_suffix = Some(format!("{}.{}", label, num));
                base = &version[..i - 1]; // exclude the dot
                break 'outer;
            }
        }
    }

    // Ensure base has 3 numeric components (MAJOR.MINOR.PATCH)
    let dot_count = base.chars().filter(|c| *c == '.').count();
    let normalized_base = match dot_count {
        0 => format!("{}.0.0", base),
        1 => format!("{}.0", base),
        _ => base.to_string(),
    };

    match pre_suffix {
        Some(pre) => format!("{}-{}", normalized_base, pre),
        None => normalized_base,
    }
}

/// Parse a Python version string as semver, with automatic normalization.
///
/// First attempts a direct semver parse. If that fails, normalizes the version
/// string to handle common Python versioning patterns before re-attempting.
fn parse_python_version(version: &str) -> Option<Version> {
    // Fast path: try direct semver parse
    if let Ok(v) = Version::parse(version) {
        return Some(v);
    }

    // Normalize and try again
    let normalized = normalize_python_version(version);
    Version::parse(&normalized).ok()
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
    package_name: &str,
    current_version: &str,
    releases: &HashMap<String, Vec<ReleaseInfo>>,
    max_versions: usize,
) -> VersionAnalysis {
    debug!("Analyzing versions for {} (current: {})", package_name, current_version);

    // Parse current version (with Python version normalization)
    let current = match parse_python_version(current_version) {
        Some(v) => v,
        None => {
            warn!("{}: failed to parse current version '{}'", package_name, current_version);
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
            // Skip version only if all release files are yanked, or if there are no files
            if release_infos.is_empty() || release_infos.iter().all(|info| info.yanked) {
                debug!("{}: skipping yanked or empty version: {}", package_name, version_str);
                return None;
            }

            // Try to parse as semver (with Python version normalization)
            match parse_python_version(version_str) {
                Some(v) => {
                    // Skip pre-release versions
                    if !v.pre.is_empty() {
                        debug!("{}: skipping pre-release version: {}", package_name, version_str);
                        return None;
                    }
                    Some(v)
                }
                None => {
                    debug!("{}: skipping unparseable version: {}", package_name, version_str);
                    None
                }
            }
        })
        .collect();

    // Sort in descending order (newest first)
    valid_versions.sort_by(|a, b| b.cmp(a));

    // Store all valid versions before truncating
    let all_valid_versions = valid_versions.clone();

    // Limit to max_versions for analysis, but ensure we keep at least one from current major
    if max_versions > 0 && valid_versions.len() > max_versions {
        valid_versions.truncate(max_versions);

        // Check if we still have a version from the current major line
        let has_current_major = valid_versions.iter().any(|v| v.major == current.major);

        if !has_current_major {
            // Try to find a version from the current major line in all valid versions
            if let Some(same_major) = all_valid_versions
                .iter()
                .find(|v| v.major == current.major && **v > current)
            {
                // Replace the last element with the same-major candidate
                if !valid_versions.is_empty() {
                    valid_versions.pop();
                    valid_versions.push(same_major.clone());
                    // Re-sort to maintain descending order
                    valid_versions.sort_by(|a, b| b.cmp(a));
                }
            }
        }
    }

    debug!("{}: valid versions after filtering: {:?}", package_name, valid_versions);

    if valid_versions.is_empty() {
        warn!("{}: no valid versions found in releases", package_name);
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

    // Find latest major version (newest version in the next major line after current)
    let latest_major = if has_update {
        // First find all major versions greater than current
        let higher_majors: Vec<_> = valid_versions
            .iter()
            .filter(|v| v.major > current.major)
            .collect();

        if !higher_majors.is_empty() {
            // Find the next major version number (smallest major > current.major)
            let next_major = higher_majors
                .iter()
                .map(|v| v.major)
                .min()
                .unwrap();

            // Find the newest version in that next major line
            higher_majors
                .iter()
                .filter(|v| v.major == next_major)
                .max_by(|a, b| a.cmp(b))
                .map(|v| v.to_string())
        } else {
            None
        }
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
        "{}: analysis complete - latest: {}, latest_major: {:?}, latest_minor: {:?}, has_update: {}",
        package_name, latest, latest_major, latest_minor, has_update
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
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

        assert_eq!(analysis.latest, "3.0.0");
        assert_eq!(analysis.latest_major, Some("3.0.0".to_string())); // Major upgrade from current (2.x -> 3.x)
        assert_eq!(analysis.latest_minor, Some("2.5.0".to_string())); // Latest in current major (2.x)
        assert!(analysis.has_update);
    }

    #[test]
    fn test_minor_update_available() {
        let releases = create_releases(&["2.5.0", "2.3.0", "2.0.5", "1.9.0"]);
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

        assert_eq!(analysis.latest, "2.5.0");
        assert_eq!(analysis.latest_major, None); // No major upgrade available (no version > 2.x)
        assert_eq!(analysis.latest_minor, Some("2.5.0".to_string()));
        assert!(analysis.has_update);
    }

    #[test]
    fn test_patch_update_available() {
        let releases = create_releases(&["2.0.5", "2.0.3", "2.0.1", "1.9.0"]);
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

        assert_eq!(analysis.latest, "2.0.5");
        assert_eq!(analysis.latest_major, None); // No major upgrade available
        assert_eq!(analysis.latest_minor, Some("2.0.5".to_string()));
        assert!(analysis.has_update);
    }

    #[test]
    fn test_already_on_latest() {
        let releases = create_releases(&["2.0.0", "1.9.0", "1.8.0"]);
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

        assert_eq!(analysis.latest, "2.0.0");
        assert_eq!(analysis.latest_major, None);
        assert_eq!(analysis.latest_minor, None);
        assert!(!analysis.has_update);
    }

    #[test]
    fn test_skip_prerelease_versions() {
        let releases = create_releases(&["3.0.0-rc1", "3.0.0-beta", "2.5.0", "2.0.0"]);
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

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
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

        assert_eq!(analysis.latest, "2.5.0");
        assert!(analysis.has_update);
    }

    #[test]
    fn test_invalid_semver_versions() {
        let mut releases = create_releases(&["2.5.0", "2.0.0"]);
        releases.insert("invalid-version".to_string(), vec![ReleaseInfo { yanked: false }]);
        releases.insert("2024.01.01".to_string(), vec![ReleaseInfo { yanked: false }]);

        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

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
        let analysis = analyze_versions("test-package", "2.0.0", &releases,5);

        // Should only consider top 5 versions: 5.0.0, 4.0.0, 3.0.0, 2.5.0, 2.4.0
        assert_eq!(analysis.latest, "5.0.0");
        assert_eq!(analysis.latest_major, Some("3.0.0".to_string())); // Newest in next major (3.x)
        assert_eq!(analysis.latest_minor, Some("2.5.0".to_string()));
    }

    #[test]
    fn test_fewer_versions_than_limit() {
        let releases = create_releases(&["2.5.0", "2.0.0"]);
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

        assert_eq!(analysis.latest, "2.5.0");
        assert!(analysis.has_update);
    }

    #[test]
    fn test_invalid_current_version() {
        let releases = create_releases(&["2.5.0", "2.0.0"]);
        let analysis = analyze_versions("test-package", "invalid", &releases, 10);

        assert_eq!(analysis.latest, "invalid");
        assert_eq!(analysis.latest_major, None);
        assert_eq!(analysis.latest_minor, None);
        assert!(!analysis.has_update);
    }

    #[test]
    fn test_no_valid_releases() {
        let releases = create_releases(&[]);
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

        assert_eq!(analysis.latest, "2.0.0");
        assert!(!analysis.has_update);
    }

    #[test]
    fn test_only_older_versions() {
        let releases = create_releases(&["1.9.0", "1.8.0", "1.7.0"]);
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

        assert_eq!(analysis.latest, "1.9.0");
        assert!(!analysis.has_update);
    }

    #[test]
    fn test_latest_minor_same_as_latest() {
        // When latest is in the same major line as current
        let releases = create_releases(&["2.5.0", "2.3.0", "1.9.0"]);
        let analysis = analyze_versions("test-package", "2.0.0", &releases,10);

        assert_eq!(analysis.latest, "2.5.0");
        assert_eq!(analysis.latest_major, None); // No major upgrade (latest is same major)
        assert_eq!(analysis.latest_minor, Some("2.5.0".to_string()));
    }

    // --- Python version normalization tests ---

    #[test]
    fn test_normalize_two_part_versions() {
        assert_eq!(normalize_python_version("0.9"), "0.9.0");
        assert_eq!(normalize_python_version("3.5"), "3.5.0");
        assert_eq!(normalize_python_version("14.0"), "14.0.0");
        assert_eq!(normalize_python_version("2025.2"), "2025.2.0");
        assert_eq!(normalize_python_version("0.89"), "0.89.0");
        assert_eq!(normalize_python_version("0.11"), "0.11.0");
    }

    #[test]
    fn test_normalize_pep440_prerelease() {
        assert_eq!(normalize_python_version("0.1.0rc3"), "0.1.0-rc.3");
        assert_eq!(normalize_python_version("0.60b1"), "0.60.0-beta.1");
        assert_eq!(normalize_python_version("1.0a1"), "1.0.0-alpha.1");
        assert_eq!(normalize_python_version("2.0.0dev1"), "2.0.0-dev.1");
        assert_eq!(normalize_python_version("1.0.0post1"), "1.0.0-post.1");
    }

    #[test]
    fn test_normalize_dot_separated_prerelease() {
        assert_eq!(normalize_python_version("3.0.0.post1"), "3.0.0-post.1");
        assert_eq!(normalize_python_version("3.1.26.post2"), "3.1.26-post.2");
        assert_eq!(normalize_python_version("0.5.0.post1"), "0.5.0-post.1");
        assert_eq!(normalize_python_version("1.0.0.rc1"), "1.0.0-rc.1");
        assert_eq!(normalize_python_version("2.0.0.dev3"), "2.0.0-dev.3");
    }

    #[test]
    fn test_normalize_already_valid_semver() {
        assert_eq!(normalize_python_version("1.2.3"), "1.2.3");
        assert_eq!(normalize_python_version("0.1.0"), "0.1.0");
        assert_eq!(normalize_python_version("12.1.0"), "12.1.0");
    }

    #[test]
    fn test_parse_python_version_two_part() {
        let v = parse_python_version("0.9").unwrap();
        assert_eq!(v, Version::parse("0.9.0").unwrap());

        let v = parse_python_version("2025.2").unwrap();
        assert_eq!(v, Version::parse("2025.2.0").unwrap());
    }

    #[test]
    fn test_parse_python_version_prerelease() {
        let v = parse_python_version("0.60b1").unwrap();
        assert!(!v.pre.is_empty());

        let v = parse_python_version("0.1.0rc3").unwrap();
        assert!(!v.pre.is_empty());
    }

    #[test]
    fn test_parse_python_version_valid_semver_passthrough() {
        let v = parse_python_version("1.2.3").unwrap();
        assert_eq!(v, Version::parse("1.2.3").unwrap());
    }

    #[test]
    fn test_two_part_current_version_analysis() {
        let releases = create_releases(&["0.10.0", "0.9.0", "0.8.0"]);
        let analysis = analyze_versions("test-package", "0.9", &releases, 10);

        assert_eq!(analysis.latest, "0.10.0");
        assert!(analysis.has_update);
        assert_eq!(analysis.latest_minor, Some("0.10.0".to_string()));
    }

    #[test]
    fn test_calendar_version_current_analysis() {
        let releases = create_releases(&["2025.3.0", "2025.2.0", "2024.12.0"]);
        let analysis = analyze_versions("test-package", "2025.2", &releases, 10);

        assert_eq!(analysis.latest, "2025.3.0");
        assert!(analysis.has_update);
    }

    #[test]
    fn test_two_part_release_versions_parsed() {
        // PyPI may return two-part release versions
        let mut releases = HashMap::new();
        releases.insert("0.10".to_string(), vec![ReleaseInfo { yanked: false }]);
        releases.insert("0.9".to_string(), vec![ReleaseInfo { yanked: false }]);
        releases.insert("0.8.0".to_string(), vec![ReleaseInfo { yanked: false }]);

        let analysis = analyze_versions("test-package", "0.8.0", &releases, 10);
        assert_eq!(analysis.latest, "0.10.0");
        assert!(analysis.has_update);
    }
}
