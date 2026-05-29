# 📦 Carrier — Universal Package Manager

Carrier is planned as a **universal package manager** designed to unify package operations across multiple language ecosystems (NuGet, Go, Pip, npm, and Cargo) under a single, flexible interface.

Rather than covering all possible package manager functionality at once, Carrier is built on a **micro-kernel architecture** that allows adding features and ecosystem providers incrementally. The project begins with a baseline boilerplate, introducing the **`outdated`** command with initial support for the **NuGet** ecosystem.

---

## 🧭 Project Vision

The long-term goal of Carrier is to provide a single tool to manage packages across different ecosystems. 

### 1. Unified Command Interface
While we start with `outdated`, Carrier is designed to eventually support core package manager actions (e.g., install, update, remove) across different project types.

### 2. Micro-Kernel Architecture
At its core, Carrier is a thin orchestrator (the kernel). All ecosystem-specific operations—such as detecting project configurations, parsing manifests/lock files, and talking to remote registries—are delegated to modular, pluggable **Providers**.

### 3. Smart Ecosystem Detection
Carrier automatically identifies the project type in the targeted directory by looking for ecosystem-specific manifest and lock files:
- **NuGet**: `.csproj`, `Directory.Packages.props` (Central Package Management)
- **npm**: `package.json`, `package-lock.json`
- **Cargo**: `Cargo.toml`, `Cargo.lock`
- **pip**: `requirements.txt`
- **Go**: `go.mod`, `go.sum`

---

## 🔍 The `outdated` Command

The first command implemented in Carrier is `outdated`. When you run `carrier outdated`, it scans the directory for direct dependencies and checks if a newer stable version exists in their respective remote registries.

### Key Rules & Behaviors:
- **Registry Comparison**: Compares the version currently in use against the latest stable version in the repository.
- **Lock File Aware**: For ecosystems that support lock files (like Cargo or npm), Carrier reads the lock file to determine the pinned version (e.g., if `Cargo.toml` specifies `"3"`, and `Cargo.lock` resolves to `3.10`, Carrier checks if there is a version newer than `3.10`, such as `3.11`). For NuGet, lock files are bypassed in favor of manifest values.
- **Wildcard and Prefix Normalization**: Properly normalizes version ranges, wildcards (e.g., `3.*` is normalized to compare with version `4`), caret (`^`), and tilde (`~`) prefixes before executing semver checks.
- **Only Stable Versions**: Standard prerelease versions (e.g. containing `-beta` or `-rc`) are filtered out when fetching the latest version.
- **Direct Dependencies & No Recursion**: Checks only direct dependencies listed in the main project manifest. Does not perform recursive dependency resolution.
- **Synchronous Execution**: The entire CLI is built using synchronous Rust (`ureq` for HTTP, standard file I/O) to keep the runtime lightweight, simple, and avoid async overhead.

---

## 🔷 Ecosystem Roadmap

| Ecosystem | Manifest File(s) | Lock File(s) | Status |
| :--- | :--- | :--- | :--- |
| **NuGet** 🔷 | `.csproj`, `Directory.Packages.props` | *None* (Ignored) | **Supported** |
| **npm** 📗 | `package.json` | `package-lock.json` | *Planned* |
| **Cargo** 🦀 | `Cargo.toml` | `Cargo.lock` | **Supported** |
| **pip** 🐍 | `requirements.txt` | *TBD* | *Planned* |
| **Go** 🐹 | `go.mod` | `go.sum` | *Planned* |

---

## 🛠️ Micro-Kernel Directory Layout

The codebase structure illustrates the micro-kernel setup:

```text
src/
├── main.rs                 # Kernel entry point
├── cli.rs                  # CLI command and option parser
├── core/
│   ├── mod.rs
│   ├── models.rs           # Shared models (Ecosystem, Dependency, OutdatedDependency)
│   ├── provider.rs         # Pluggable Provider trait definition
│   └── orchestrator.rs     # Core kernel engine (runs detection and parses updates)
├── output/
│   ├── mod.rs
│   └── table.rs            # Simple list output formatting
└── providers/
    ├── mod.rs              # Registration list for the kernel
    ├── nuget/              # NuGet provider module (CPM and csproj support)
    │   ├── mod.rs
    │   ├── parser.rs       # Parsing XML elements (PackageReference / PackageVersion)
    │   └── registry.rs     # Synchronous NuGet v3 API communication
    └── cargo/              # Cargo provider module (Cargo.toml and Cargo.lock support)
        ├── mod.rs
        ├── parser.rs       # Parsing TOML dependencies and lock entries
        └── registry.rs     # Synchronous crates.io API communication
```

---

## 🚀 Running Carrier

### Prerequisites
You need the Rust toolchain installed. Install it via [rustup](https://rustup.rs/).

### Build and Run

1. Clone the repository:
   ```bash
   git clone https://github.com/your-username/carrier.git
   cd carrier
   ```
2. Build the project:
   ```bash
   cargo build --release
   ```
3. Run the `outdated` command on the current directory:
   ```bash
   # Using cargo
   cargo run -- outdated

   # Or run the built binary
   ./target/release/carrier outdated
   ```

To scan a specific path:
```bash
./target/release/carrier outdated --path /path/to/project
```

---

## 🧪 Testing

To run the unit tests:
```bash
cargo test
```

---

## ➕ Adding a New Provider

The micro-kernel makes it straightforward to add support for a new package manager:

1. **Implement the `Provider` trait** (`src/core/provider.rs`) for your ecosystem. You'll define how to:
   - Identify the ecosystem and provider name.
   - Detect matching manifest/lock files in a directory.
   - Parse direct dependencies (resolving actual versions using lock files if applicable).
   - Query the registry synchronously for the latest stable version of a package.
2. **Register the provider** inside `src/providers/mod.rs` inside the `register_all_providers` function:
   ```rust
   registry.register(Box::new(npm::NpmProvider::new()));
   ```
