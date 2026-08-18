# 🧹 CLI-NER

> **Advanced, safe, and documented CLI for macOS disk space management and cleanup.**

[![CI](https://github.com/fabrizioriccardo73/cli-ner/actions/workflows/ci.yml/badge.svg)](https://github.com/fabrizioriccardo73/cli-ner/actions/workflows/ci.yml)
[![GitHub release](https://img.shields.io/github/v/release/fabrizioriccardo73/cli-ner?include_prereleases&style=flat-square)](https://github.com/fabrizioriccardo73/cli-ner/releases)
[![Homebrew](https://img.shields.io/badge/homebrew-fabrizioriccardo73%2Ftap-orange?style=flat-square&logo=homebrew)](https://github.com/fabrizioriccardo73/homebrew-tap)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](https://opensource.org/licenses/MIT)
[![Platform: macOS](https://img.shields.io/badge/platform-macOS-lightgrey?style=flat-square&logo=apple)](https://apple.com/macos)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)

[![CsXaLjs.md.jpg](https://iili.io/CsXaLjs.md.jpg)](https://freeimage.host/i/CsXaLjs)

---

## 🚀 Installation & Quick Start

`cli-ner` is designed for macOS (Apple Silicon M1/M2/M3/M4 & Intel).

### Option 1: Homebrew (Recommended)

Install via the official Homebrew tap:

```bash
# Add tap and install cli-ner
brew install fabrizioriccardo73/tap/cli-ner
```

To update in the future:
```bash
brew upgrade cli-ner
```

---

### Option 2: Quick Install Script (Standalone)

Download and install the latest precompiled release binary automatically:

```bash
curl -fsSL https://raw.githubusercontent.com/fabrizioriccardo73/cli-ner/master/install.sh | bash
```

---

### Option 3: Precompiled Binaries (Manual)

Download the pre-built tarball for your Mac architecture directly from the [GitHub Releases](https://github.com/fabrizioriccardo73/cli-ner/releases) page:
- `cli-ner-vX.Y.Z-aarch64-apple-darwin.tar.gz` (Apple Silicon M1/M2/M3/M4)
- `cli-ner-vX.Y.Z-x86_64-apple-darwin.tar.gz` (Intel Mac)

Extract and move the binary to your `PATH`:
```bash
tar -xzf cli-ner-*-apple-darwin.tar.gz
sudo mv cli-ner /usr/local/bin/
```

---

### Option 4: Cargo (From Source)

If you have Rust and Cargo installed:

```bash
# Install directly from GitHub:
cargo install --git https://github.com/fabrizioriccardo73/cli-ner

# Or install from a local clone:
cargo install --path .
```

---

## 🌟 Key Features

- 🛡️ **Safety-First & Reversible by Default**:
  - Moves files to **macOS Trash** (`~/.Trash`) instead of permanently deleting them.
  - **Active Browser Protection**: Automatically detects running browsers (Chrome, Safari, Firefox, Brave, Edge, Arc, etc.) and safely **excludes** their cache folders to prevent corrupted tabs, broken CSS/JS, or extension errors.
  - **Dry-Run Mode enabled by default**: see exactly what would be cleaned and how much space would be reclaimed before performing any action.
  - **Strict Blocklist**: Critical system directories (`/System`, `/usr`, `/bin`, etc.) and personal user data (`~/Documents`, `~/Desktop`, `~/.ssh`, `~/Library/Mail`, `~/Library/Keychains`, etc.) are **NEVER** touched.
  - **Controlled Allowlist**: Cleans only explicitly authorized safe targets (`~/Library/Caches/*`, `~/Library/Logs/*`, `/tmp/*`, developer caches) without ever deleting root folders.
- ⚡ **High Performance**:
  - Written in **Rust**, fast and resource-efficient for recursive filesystem scanning and size calculations.
- 🛠️ **Developer Tools & Caches Support**:
  - **Homebrew**: `brew cleanup -s`, `brew autoremove`
  - **Node.js**: `npm` cache (`~/.npm/_cacache`)
  - **Python**: `pip` cache (`~/Library/Caches/pip`)
  - **Docker**: Accurate space detection via `docker system df`, container data loss warnings, build cache, dangling images, and stopped containers.
  - **Xcode**: DerivedData, Archives, iOS DeviceSupport (with process checks to ensure Xcode is not running).
- 📝 **Immutable Audit Trail & Logging**:
  - Every scan and clean operation is logged to `~/.cli-ner/logs/` in JSON Lines format with timestamps, freed bytes, processed item lists, and errors.
- 🖥️ **Interactive Terminal UI (TUI) Dashboard**:
  - Full graphical terminal dashboard powered by `ratatui` to explore logs, inspect item-by-item operations, and view reclaimed space breakdown charts.
- 🔍 **Disk Analyzer & Large Files Finder**:
  - Directory space usage mapping with percentage breakdown.
  - Recursive search for large files exceeding a customizable threshold (e.g., `--min-size 500MB`).

---

## 🧭 Recommended Workflow

1. **System Health & Diagnostics**:
   ```bash
   cli-ner doctor
   ```
2. **Interactive Docker Inspection & Safe Cleanup**:
   ```bash
   cli-ner docker
   ```
3. **Simulate Cache & Developer Cleanup (Dry-Run)**:
   ```bash
   cli-ner clean
   ```
4. **Execute Safe Cleanup (Moves files to macOS Trash)**:
   ```bash
   cli-ner clean --execute
   ```
5. **Explore History & Metrics (TUI Dashboard)**:
   ```bash
   cli-ner dashboard
   ```

---

## 📖 Usage & Commands

### 1. `cli-ner scan` — Disk Space Analysis
```bash
# Analyze current directory or user home directory
cli-ner scan

# Analyze a specific path
cli-ner scan --path ~/Downloads

# Display top 20 items by size
cli-ner scan --top 20

# Search for large files (>= 500MB)
cli-ner scan --large-files --min-size 500MB

# Structured JSON output
cli-ner scan --format json
```

### 2. `cli-ner clean` — Safe Cache & Temp Cleanup
```bash
# Dry-run (DEFAULT): Preview what will be cleaned WITHOUT modifying any files
cli-ner clean

# Execute actual cleanup (moves files to macOS Trash)
cli-ner clean --execute

# Target a specific category
cli-ner clean --category user-cache --execute
cli-ner clean --category xcode --execute
cli-ner clean --category docker --execute
cli-ner clean --category dev --execute

# Permanent deletion (requires explicit flag and confirmation)
cli-ner clean --execute --force
```

### 3. `cli-ner dashboard` (or `cli-ner report --tui`) — Interactive TUI Dashboard
```bash
# Launch interactive terminal UI dashboard to explore audit logs & statistics
cli-ner dashboard

# Or launch via report flag
cli-ner report --tui
```
**Dashboard Keybindings**:
- `[1] / [2] / [3]` or `[Tab]`: Switch between Operations History, Operation Details, and Category Statistics.
- `[↑] / [↓]` or `[j] / [k]`: Navigate operations or individual cleaned files.
- `[Enter]` or `[d]`: Inspect details for the selected entry.
- `[q]` or `[Esc]`: Quit dashboard.

### 4. `cli-ner report` — Audit Logs & History
```bash
# Display summary table of the last 10 operations
cli-ner report

# Show detailed breakdown for the last operation
cli-ner report --last

# Export to JSON format
cli-ner report --format json
```

### 5. `cli-ner docker` — Safe Interactive Docker Manager & Cleanup
```bash
# Launch the interactive Docker management & cleanup wizard
cli-ner docker

# Run guided step-by-step cleanup wizard in dry-run mode
cli-ner docker wizard --dry-run

# Inspect and manage stopped containers (active containers are protected!)
cli-ner docker containers
cli-ner docker containers --list

# Inspect and manage images (in-use images are locked and protected)
cli-ner docker images
cli-ner docker images --list
cli-ner docker images --dangling

# Purge BuildKit build cache
cli-ner docker build-cache

# Run safety audit on persistent volumes (database & application data)
cli-ner docker volumes

# Storage breakdown summary (docker system df)
cli-ner docker status
```

**Docker Safety Guarantees**:
- 🟢 **Running Containers**: Locked and protected against accidental removal.
- 🔒 **In-Use Images**: Cross-referenced with active containers and protected from deletion.
- ⚠️ **Volumes & Persistent Data**: Volumes are **NEVER** deleted in standard cleanups. Container mounts (databases, bind-mounts) are clearly displayed before any stopped container removal.

### 6. `cli-ner doctor` — System Diagnostics & Environment Check

```bash
cli-ner doctor
```
Checks:
- Available and total space across all mounted disks.
- Availability of external tools (Homebrew, npm, pip, Docker, Xcode).
- Status of security protections, permissions, and paths.

---

## 🛡️ Detailed Safety Model

### 5-Step Validation Pipeline

Every file or directory candidate for cleanup passes through the following validation checks before any action is taken:

1. **Blocklist Check**: If the target belongs to critical system paths (`/System`, `/usr`, etc.) or personal user data (`~/.ssh`, `~/Documents`, `~/Desktop`, `~/Library/Mail`), the operation is **immediately rejected**.
2. **Allowlist Verification**: The path must strictly belong to an authorized cleanable category.
3. **Root Folder Protection**: Deleting root folders (e.g., `~/Library/Caches`) is prohibited; only their individual child items can be cleaned.
4. **Symlink Guard**: Symlinks are never blindly followed to prevent unintended deletions outside target directories.
5. **Process Safety Check**: Sensitive operations (e.g., Xcode DerivedData) verify that the corresponding application is not currently active.

---

## 🧪 Running Tests

The project includes a comprehensive unit and end-to-end integration test suite:

```bash
# Run all tests
cargo test

# Run tests with detailed output
cargo test -- --nocapture
```

---

## 🤝 Contributing & Community

Contributions, issues, and feature requests are welcome!

- ⭐️ **Star the repository**: If you find `cli-ner` useful, give it a star on [GitHub](https://github.com/fabrizioriccardo73/cli-ner)!
- 🐛 **Report a Bug**: Open an issue using our [Bug Report Template](https://github.com/fabrizioriccardo73/cli-ner/issues/new?template=bug_report.yml).
- 💡 **Suggest a Feature**: Propose a new cache cleanup target using our [Feature Request Template](https://github.com/fabrizioriccardo73/cli-ner/issues/new?template=feature_request.yml).
- 🛠️ **Submit a PR**: Read [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md) for development workflows and safety requirements.

---

## 📄 License

Distributed under the MIT License. See [LICENSE](LICENSE) for details.

