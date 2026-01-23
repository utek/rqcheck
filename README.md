# rqcheck

A robust, high-performance command-line tool for checking Python package version updates against PyPI.

`rqcheck` parses your `requirements.txt` files and concurrently checks PyPI for the latest available versions of your dependencies. It helps you keep your Python projects up-to-date with minimal effort.

## Features

- **High Performance**: Uses async/await and concurrent batch processing to check hundreds of packages in seconds.
- **Robust Error Handling**: resilient against network issues with built-in retry logic and exponential backoff.

## Usage

### Basic Check
Check a requirements file for updates:

```bash
rqcheck -f requirements.txt
```

```bash
rqcheck -f requirements.in
```

### Common Options

```bash
# Set a custom batch size for concurrency (default: 10)
rqcheck -f requirements.txt -b 20

# Enable verbose logging to see progress
rqcheck -f requirements.txt -v

# Specify a config file
rqcheck -f requirements.txt -c my_config.toml
```

### Configuration

You can configure `rqcheck` using a `rqcheck.toml` file:

```toml
batch_size = 15
timeout_seconds = 60
max_retries = 5
pypi_base_url = "https://pypi.org/pypi"
```

Environment variables are also supported (e.g., `RQCHECK_BATCH_SIZE`, `RQCHECK_TIMEOUT`).

## Installation

Building from source requires a Rust toolchain:

```bash
cargo build --release
```

The binary will be available in `target/release/rqcheck`.

## minimal requirements file example

```txt
requests==2.28.0
flask==2.0.0
numpy==1.21.0
```

## License

MIT
