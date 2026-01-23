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
        .with_body(r#"{"info": {"version": "2.32.0"}}"#)
        .create_async()
        .await;

    let _m2 = server.mock("GET", "/flask/json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"info": {"version": "3.0.0"}}"#)
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

    config.apply_cli_overrides(Some(20), None, false);
    assert_eq!(config.batch_size, 20); // Overridden
}
