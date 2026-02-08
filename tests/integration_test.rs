use rqcheck::checker::{check_packages_batched, PackageRequirement};
use rqcheck::config::Config;
use mockito::Server;
use tempfile::NamedTempFile;
use std::io::Write;

#[tokio::test]
async fn test_e2e_with_mock_pypi() {
    let mut server = Server::new_async().await;
    
    let _m1 = server.mock("GET", "/requests/json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"info": {"version": "2.32.0"}, "releases": {"2.32.0": [{"yanked": false}], "2.31.0": [{"yanked": false}], "2.28.0": [{"yanked": false}]}}"#)
        .create_async()
        .await;

    let _m2 = server.mock("GET", "/flask/json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"info": {"version": "3.0.0"}, "releases": {"3.0.0": [{"yanked": false}], "2.3.0": [{"yanked": false}]}}"#)
        .create_async()
        .await;

    let requirements = vec![
        PackageRequirement {
            name: "requests".to_string(),
            version: "2.28.0".to_string(),
        },
        PackageRequirement {
            name: "flask".to_string(),
            version: "2.3.0".to_string(),
        },
    ];

    let mut config = Config::default();
    config.pypi_base_url = server.url();
    config.batch_size = 10;

    let results = check_packages_batched(requirements, &config).await;

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.has_update));
}

#[test]
fn test_config_file_loading() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        r#"
batch_size = 25
timeout_seconds = 60
max_retries = 5
"#
    )
    .unwrap();

    let config = Config::load(Some(temp_file.path().to_str().unwrap())).unwrap();
    assert_eq!(config.batch_size, 25);
    assert_eq!(config.timeout_seconds, 60);
    assert_eq!(config.max_retries, 5);
}

#[test]
fn test_cli_batch_size_override() {
    let mut config = Config::default();
    assert_eq!(config.batch_size, 10); // Default

    config.apply_cli_overrides(Some(20), None, None, false);
    assert_eq!(config.batch_size, 20); // Overridden
}

#[tokio::test]
async fn test_version_analysis_major_update() {
    let mut server = Server::new_async().await;

    // Django with major version jump available
    let _m = server.mock("GET", "/django/json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "info": {"version": "5.0.0"},
            "releases": {
                "5.0.0": [{"yanked": false}],
                "4.2.8": [{"yanked": false}],
                "4.2.0": [{"yanked": false}],
                "3.2.23": [{"yanked": false}],
                "3.2.0": [{"yanked": false}]
            }
        }"#)
        .create_async()
        .await;

    let requirements = vec![
        PackageRequirement {
            name: "django".to_string(),
            version: "3.2.0".to_string(),
        },
    ];

    let mut config = Config::default();
    config.pypi_base_url = server.url();

    let results = check_packages_batched(requirements, &config).await;

    assert_eq!(results.len(), 1);
    assert!(results[0].has_update);
    assert_eq!(results[0].latest_version, "5.0.0");
    assert_eq!(results[0].latest_major_version, Some("4.2.8".to_string()));
    assert_eq!(results[0].latest_minor_version, Some("3.2.23".to_string()));
}

#[tokio::test]
async fn test_version_analysis_skip_prerelease() {
    let mut server = Server::new_async().await;

    // Package with pre-release versions that should be skipped
    let _m = server.mock("GET", "/package/json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "info": {"version": "3.0.0-rc1"},
            "releases": {
                "3.0.0-rc1": [{"yanked": false}],
                "3.0.0-beta": [{"yanked": false}],
                "2.5.0": [{"yanked": false}],
                "2.0.0": [{"yanked": false}]
            }
        }"#)
        .create_async()
        .await;

    let requirements = vec![
        PackageRequirement {
            name: "package".to_string(),
            version: "2.0.0".to_string(),
        },
    ];

    let mut config = Config::default();
    config.pypi_base_url = server.url();

    let results = check_packages_batched(requirements, &config).await;

    assert_eq!(results.len(), 1);
    assert!(results[0].has_update);
    // Should skip pre-release and use 2.5.0 as latest
    assert_eq!(results[0].latest_version, "2.5.0");
}

#[tokio::test]
async fn test_version_analysis_skip_yanked() {
    let mut server = Server::new_async().await;

    // Package with yanked version
    let _m = server.mock("GET", "/package/json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "info": {"version": "3.0.0"},
            "releases": {
                "3.0.0": [{"yanked": true}],
                "2.5.0": [{"yanked": false}],
                "2.0.0": [{"yanked": false}]
            }
        }"#)
        .create_async()
        .await;

    let requirements = vec![
        PackageRequirement {
            name: "package".to_string(),
            version: "2.0.0".to_string(),
        },
    ];

    let mut config = Config::default();
    config.pypi_base_url = server.url();

    let results = check_packages_batched(requirements, &config).await;

    assert_eq!(results.len(), 1);
    assert!(results[0].has_update);
    // Should skip yanked 3.0.0 and use 2.5.0 as latest
    assert_eq!(results[0].latest_version, "2.5.0");
}

#[tokio::test]
async fn test_max_versions_configuration() {
    let mut server = Server::new_async().await;

    // Package with many versions
    let _m = server.mock("GET", "/package/json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "info": {"version": "5.0.0"},
            "releases": {
                "5.0.0": [{"yanked": false}],
                "4.0.0": [{"yanked": false}],
                "3.0.0": [{"yanked": false}],
                "2.5.0": [{"yanked": false}],
                "2.4.0": [{"yanked": false}],
                "2.3.0": [{"yanked": false}],
                "2.2.0": [{"yanked": false}],
                "2.1.0": [{"yanked": false}],
                "2.0.5": [{"yanked": false}],
                "2.0.0": [{"yanked": false}],
                "1.9.0": [{"yanked": false}]
            }
        }"#)
        .create_async()
        .await;

    let requirements = vec![
        PackageRequirement {
            name: "package".to_string(),
            version: "2.0.0".to_string(),
        },
    ];

    let mut config = Config::default();
    config.pypi_base_url = server.url();
    config.max_versions_to_check = 5; // Only check top 5 versions

    let results = check_packages_batched(requirements, &config).await;

    assert_eq!(results.len(), 1);
    assert!(results[0].has_update);
    assert_eq!(results[0].latest_version, "5.0.0");
}

#[tokio::test]
async fn test_already_on_latest() {
    let mut server = Server::new_async().await;

    // Package already on latest version
    let _m = server.mock("GET", "/package/json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "info": {"version": "2.5.0"},
            "releases": {
                "2.5.0": [{"yanked": false}],
                "2.4.0": [{"yanked": false}],
                "2.3.0": [{"yanked": false}]
            }
        }"#)
        .create_async()
        .await;

    let requirements = vec![
        PackageRequirement {
            name: "package".to_string(),
            version: "2.5.0".to_string(),
        },
    ];

    let mut config = Config::default();
    config.pypi_base_url = server.url();

    let results = check_packages_batched(requirements, &config).await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].has_update);
    assert_eq!(results[0].latest_version, "2.5.0");
    assert_eq!(results[0].latest_major_version, None);
    assert_eq!(results[0].latest_minor_version, None);
}
