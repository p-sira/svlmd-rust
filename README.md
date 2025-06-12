# SVLMD-rust

Backend utilities for Sira's Very Large Medical Database (SVLMD). This Rust implementation provides robust file management and version control features for Logseq-based medical database management.

Main repository: [SVLMD](https://github.com/p-sira/SVLMD)

## Features

- **Initialization System**: Set up SVLMD with contributor information
- **Version Control Integration**: Track changes in Logseq pages with Git integration
- **File Management**: Robust handling of Logseq pages with property and content management
- **Sync System**: Synchronize database versions and track changes
- **Contributor Management**: Track and manage contributor information

## Installation

### Prerequisites

- Rust (latest stable version)
- Git

### Building from Source

1. Clone the repository:
```bash
git clone https://github.com/p-sira/svlmd-rust.git
cd svlmd-rust
```

2. Build the project:
```bash
cargo build --release
```

The compiled binary will be available in `target/release/svlmd`.

## Usage

### Initialize SVLMD

To set up SVLMD in your project:

```bash
svlmd init
```

This will:
- Create a `.svlmd` configuration file
- Prompt for contributor information
- Set up necessary Logseq page structures

### Sync Database

To synchronize the database and track changes:

```bash
svlmd sync
```

Options:
- `-V, --version`: Sync version metadata
- `-v, --verbose`: Enable verbose output

## Project Structure

- `src/main.rs`: Core CLI implementation and command handling
- `src/file_manager.rs`: File management and Logseq page handling utilities

## License

This project is licensed under the BSD 3-Clause License. See [LICENSE](LICENSE).

## Author

Sira Pornsiriprasert <code@psira.me>