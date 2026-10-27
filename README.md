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

## 📚 Documentation

| Document | What's in it |
|---|---|
| **[Usage Guide & Best Practices](docs/GUIDA_UTILIZZO.md)** 🇮🇹 | The recommended routine, how to read the reported numbers, category reference, how to restore trashed items, troubleshooting, and what to automate — and what never to. |
| [Distribution Guide](docs/DISTRIBUTION_GUIDE.md) | Release, packaging and Homebrew tap workflow. |
| [Contributing](CONTRIBUTING.md) | Development setup and PR conventions. |
| [Security Policy](SECURITY.md) | Reporting vulnerabilities. |

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
  - **In-use temp files are left alone**: entries in `/private/tmp` and `/private/var/tmp` are only eligible once untouched for 24 hours, so scratch files and sockets belonging to running processes are never pulled out from under them.
- 📏 **Numbers You Can Trust**:
  - **"Moved to Trash" and "deleted" are reported separately.** A move to `~/.Trash` frees nothing until the Trash is emptied, and cli-ner says so instead of counting it as reclaimed space.
  - **The real disk delta is measured** before and after an execution and printed alongside the logical totals.
  - **Overlapping targets are counted once.** `~/Library/Caches` and the package-manager caches inside it never contribute the same gigabytes twice.
  - **Sizes are complete.** Nothing is silently skipped while measuring, so the previewed size matches what actually gets removed.
  - **External tools are re-measured, not assumed.** `brew cleanup`, `npm cache clean`, `pip cache purge` and `docker prune` decide for themselves what to evict; cli-ner reports what actually went, not the size of the directory it hoped to empty.
  - **Unmeasurable things say so.** APFS local snapshot sizes cannot be queried on macOS, so they are listed as `not measurable` and excluded from totals rather than guessed at.
  - **Blocked locations are surfaced.** If macOS privacy protection hides a directory, cli-ner tells you the reported total is an undercount and how to fix it.
- ♻️ **Restorable**: every trashed item's original path *and* its location inside `~/.Trash` are recorded in the audit log — see them with `cli-ner report --last`.
- ⚡ **High Performance**:
  - Written in **Rust**, with parallel directory measurement and a single scan per run (the preview you approve is the exact set that gets cleaned).
- 🛠️ **Developer Tools & Caches Support**:
  - **Homebrew**: `brew cleanup -s`, `brew autoremove`
  - **Node.js**: `npm` cache (`~/.npm/_cacache`), **pnpm** store (`pnpm store prune`)
  - **Python**: `pip` cache (`~/Library/Caches/pip`)
  - **Rust**: `~/.cargo/registry/cache`, extracted sources in `registry/src`, and git dependency checkouts
  - **Go**: module cache via `go clean -modcache`
  - **Gradle/Maven**: dependency caches plus stale Gradle wrapper distributions
  - **Docker**: Accurate space detection via `docker system df`, container data loss warnings, build cache, dangling images, and stopped containers.
  - **Xcode**: DerivedData, Archives, iOS DeviceSupport (with process checks to ensure Xcode is not running), CoreSimulator caches, and simulator devices whose runtime is no longer installed.
- 📦 **Universal "Project Sweep" & Dormant Project Artifacts**:
  - Automatic detection of software projects across your machine (Rust, Node.js, Python, Gradle, Composer, Go, Flutter).
  - Inactivity and dormancy calculation (via Git commits / source file timestamps, e.g. 60+ days).
  - Interactive multi-select UI to selectively clean `node_modules`, `target/`, `.venv`, `.gradle/`, `vendor/`, `.dart_tool/`, etc. with mandatory confirmation.
- 📝 **Immutable Audit Trail & Logging**:
  - Every scan and clean operation is logged to `~/.cli-ner/logs/` in JSON Lines format with timestamps, freed bytes, processed item lists, and errors.
- 🖥️ **Interactive Terminal UI (TUI) Dashboard**:
  - Full graphical terminal dashboard powered by `ratatui` to explore logs, inspect item-by-item operations, and view reclaimed space breakdown charts.
- 🔍 **Disk Analyzer & Large Files Finder**:
  - Directory space usage mapping with percentage breakdown.
  - Recursive search for large files exceeding a customizable threshold (e.g., `--min-size 500MB`).
- 🔬 **Phantom Disk Space Bloat Analyzer (`cli-ner bloat` / `cli-ner phantom`)**:
  - Uncovers hidden causes behind "vanishing" disk space on macOS: Time Machine local snapshots, sleep image (`/var/vm/sleepimage`), Docker VM disks, iOS simulator runtimes, CloudStorage provider caches, browser profiles, and ASL/system logs.
  - Categorizes severity with actionable recovery hints and commands.

---

## 🧭 Recommended Workflow

1. **System Health & Diagnostics**:
   ```bash
   cli-ner doctor
   ```
2. **Diagnose "Invisible" Phantom Disk Space**:
   ```bash
   cli-ner bloat
   ```
3. **Interactive Docker Inspection & Safe Cleanup**:
   ```bash
   cli-ner docker
   ```
4. **Simulate Cache & Developer Cleanup (Dry-Run)**:
   ```bash
   cli-ner clean
   ```
5. **Execute Safe Cleanup (Moves files to macOS Trash)**:
   ```bash
   cli-ner clean --execute
   ```
6. **Actually Reclaim the Space** — moving to the Trash frees nothing until it is emptied.
   Verify everything still works first, then:
   ```bash
   cli-ner clean --category trash --execute
   ```
7. **Explore History & Metrics (TUI Dashboard)**:
   ```bash
   cli-ner dashboard
   ```

> 📘 **New to CLI-NER?** The [Usage Guide & Best Practices](docs/GUIDA_UTILIZZO.md) (Italian)
> covers the recommended routine, how to read the reported numbers, how to restore trashed
> items, and what to automate — and what never to.

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

# Pick item by item from a checklist (nothing is pre-selected)
cli-ner clean --interactive
cli-ner clean --interactive --execute

# Target a specific category
cli-ner clean --category user-cache --execute
cli-ner clean --category xcode --execute
cli-ner clean --category docker --execute
cli-ner clean --category dev --execute

# Skip the confirmation prompt (for scripts)
cli-ner clean --category npm --execute --yes

# Permanent deletion (requires explicit flag and confirmation)
cli-ner clean --execute --force

# Empty the Trash — this is what actually returns the space to the volume
cli-ner clean --category trash --execute
```

**The preview is the contract.** The items shown in the dry-run table are the exact items that
get cleaned — the target list is scanned once and carried through selection, confirmation and
execution, so nothing can drift in between.

#### Available categories

| Category | Aliases | Targets |
|---|---|---|
| `all` | | Everything below except `biome` (**default**) |
| `app-caches` | `deep` | extended-app-cache + app-support-cache + container-cache + metal-cache |
| `dev` | `dev-tools`, `developer` | brew + npm + pnpm + pip + gradle + maven + cargo + go |
| `xcode` | | DerivedData + Archives + DeviceSupport + Simulators |
| `user-cache` | `cache` | `~/Library/Caches/*` |
| `user-logs` | `logs` | `~/Library/Logs/*` |
| `temp-files` | `temp` | `/private/tmp`, `/private/var/tmp` (only entries idle for 24h+) |
| `trash` | | Empties `~/.Trash` — **always permanent** |
| `docker` | | Build cache + dangling images + stopped containers |
| `brew` | `homebrew` | `brew cleanup -s` + `brew autoremove` |
| `npm` | | `npm cache clean --force` |
| `pnpm` | | `pnpm store prune` |
| `pip` | | `pip3 cache purge` |
| `gradle` | | `~/.gradle/caches` + stale wrapper distributions |
| `maven` | | `~/.m2/repository` |
| `cargo` | | Registry cache + extracted sources + git checkouts |
| `go` | `golang` | `go clean -modcache` |
| `xcode-derived-data` | | DerivedData only |
| `xcode-archives` | `archives` | Build archives only |
| `xcode-device-support` | `device-support` | iOS symbol files only |
| `xcode-simulator` | `simulator` | CoreSimulator caches + devices with an uninstalled runtime |
| `extended-app-cache` | `extended-cache` | Downloaded wallpaper/screensaver assets, fontconfig cache |
| `app-support-cache` | `electron` | Electron/Chromium apps' `~/Library/Application Support/<app>/{Cache, Code Cache, GPUCache, Dawn*Cache, …}` (running apps skipped) |
| `container-cache` | `containers` | `~/Library/Containers/<app>/Data/Library/Caches` (running apps, Mail and Messages skipped) |
| `metal-cache` | `metal` | Compiled Metal shaders under `/var/folders` (shared and per-app) |
| `biome` | | `~/Library/Biome` — **personal data**, never part of `all`; needs Full Disk Access |

Everything CLI-NER removes is **regenerable**: it is re-downloaded or rebuilt on the next build.
The cost of a mistake is time, never data. The one exception is `biome`, which must be asked for
by name: it is the on-device app-usage history macOS uses for suggestions, which start over.

Root-owned system caches (coresymbolicationd, IntlDataCache, print spooler, core dumps) are not in
`clean`: they live in `cli-ner maintain --allow-admin`, which asks for your password once through
the native `sudo` prompt.

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

`cli-ner report --last` includes a **"Recover From"** column: for every trashed item you get
both its original path and where it now sits inside `~/.Trash`. A direct POSIX rename into the
Trash carries no Finder "Put Back" metadata, so this log is what makes a restore possible.

Logs are JSON Lines under `~/.cli-ner/logs/operations_YYYY-MM-DD.jsonl` and stay readable with
`jq` independently of CLI-NER.

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

### 6. `cli-ner projects` (or `cli-ner sweep`) — Dormant Project Sweep & Build Artifacts Cleaner

Universal project scanner (like an all-ecosystem `npkill`) that detects software projects on your disk, calculates inactivity based on Git commits/source files, and safely cleans heavy build artifacts:

```bash
# Preview dormant projects (inactivity >= 60 days) in common dev folders or cwd
cli-ner projects

# Interactive selection mode with checkbox list
cli-ner projects --interactive
# or shortcut:
cli-ner projects -i

# Scan a specific directory and filter by days of inactivity (e.g. >= 30 days)
cli-ner projects --path ~/development --days 30

# Filter by software ecosystem (node, rust, python, gradle, composer, go, flutter)
cli-ner projects --type node
cli-ner projects --type rust

# Include all projects (even active ones) with a minimum size threshold
cli-ner projects --all --min-size 100MB

# Execute cleaning of dormant project artifacts (moves to macOS Trash with confirmation)
cli-ner projects --execute

# Permanent deletion (requires explicit confirmation)
cli-ner projects --execute --force
```

**Supported Ecosystems & Build Artifacts**:
- 🦀 **Rust**: `target/`
- 🟢 **Node.js**: `node_modules/`, `.next/`, `.nuxt/`, `.turbo/`
- 🐍 **Python**: `.venv/`, `venv/`, `__pycache__/`, `.pytest_cache/`, `.ruff_cache/`, `.mypy_cache/`
- ☕ **Java / Kotlin**: `.gradle/`, `build/`, `target/` (Maven)
- 🐘 **PHP / Composer**: `vendor/`
- 🐹 **Go**: `vendor/`
- 💙 **Flutter / Dart**: `.dart_tool/`, `build/`

### 7. `cli-ner doctor` — System Diagnostics & Environment Check

```bash
cli-ner doctor
```
Checks:
- Available and total space across all mounted disks.
- Availability of external tools (Homebrew, npm, pip, Docker, Xcode).
- **Full Disk Access**: whether `~/.Trash`, `~/Library/Caches`, `~/Library/Logs` and
  `~/Library/Developer` are actually readable. Anything macOS blocks is silently excluded from
  every scan, so `doctor` tells you when the reported totals are an undercount and how to fix it.
- Running browsers whose caches will be protected during cleanup.
- Status of security protections, permissions, and paths.

> Run this first on a new machine. Without Full Disk Access the Trash is usually invisible, and
> every total CLI-NER prints will be lower than reality.

### 8. `cli-ner bloat` (or `cli-ner phantom`) — Phantom Disk Space Analyzer

Diagnose where 20-50+ GB of "invisible" disk space has disappeared on macOS:

```bash
# Analyze all phantom bloat sources on your Mac
cli-ner bloat

# Or use the shortcut alias:
cli-ner phantom

# Show detailed filesystem paths, file counts, and diagnostic hints
cli-ner bloat --detailed

# Filter by minimum size threshold (e.g., >= 500MB or >= 1GB)
cli-ner bloat --min-size 500MB

# Also scan privileged system locations (/Library, /var/log, /var/vm, Spotlight index)
cli-ner bloat --system

# Structured JSON output
cli-ner bloat --format json
```

**How the total is computed**: scanners deliberately overlap — `~/Library/Caches` is reported
as a whole *and* broken down per package manager. Nested locations are counted **once**, so the
headline figure is not the naive sum of the rows. Rows whose size macOS cannot report (APFS
local snapshots) are shown as `not measurable` and excluded from the total rather than guessed at.

`--system` only *reports* privileged locations along with the `sudo` command to reclaim them.
CLI-NER never runs anything as root.

**What it detects**:
- 🕐 **Time Machine Local Snapshots**: APFS snapshots locking freed blocks.
- 💤 **Sleep Image & Swap**: `/var/vm/sleepimage` (RAM dump) and swapfiles.
- ☁️ **Cloud Storage Caches**: `~/Library/CloudStorage` (OneDrive, Google Drive, Dropbox cached offline copies) and iCloud Mobile Documents.
- 🐳 **Docker & Container VM Disks**: `Docker.raw` / OrbStack data.
- 🔨 **Xcode & Simulators**: DerivedData, DeviceSupport, and iOS Simulator disk runtimes (`xcrun simctl`).
- 🌐 **Browser Storage**: Chrome, Edge, Safari, Arc, Brave profile data & caches.
- 📋 **System & ASL Logs**: `/var/log`, `/Library/Logs`, `~/Library/Logs`.
- 📦 **Dev Package Caches**: Homebrew, Gradle, Maven, npm, pnpm, pip, CocoaPods, Cargo caches.

> ⚠️ **Browser rows are informational only — `clean` never touches them.** `bloat` measures
> `~/Library/Application Support/<Browser>` (or `~/Library/Safari`), which holds bookmarks,
> history, saved passwords, cookies, extensions and site storage — real data, not cache. It is
> intentionally outside the allowlist. The browser's *actual* HTTP cache lives separately under
> `~/Library/Caches/<Browser>` and **is** covered by `cli-ner clean --category user-cache` (close
> the browser first). To reclaim the rest, use the browser's own "Clear browsing data" and remove
> unused profiles — see [the usage guide](docs/GUIDA_UTILIZZO.md#9-browser-cache-vs-dati-personali)
> for the safe, step-by-step version.

---

### 9. `cli-ner snapshot` — Point-in-Time Disk Space Snapshots

Take snapshots of your disk usage state to track how disk consumption evolves over time:

```bash
# Capture a new disk snapshot of your home directory
cli-ner snapshot create

# Or create a snapshot with a descriptive label
cli-ner snapshot create --name "before-xcode-build"

# List all saved historical snapshots
cli-ner snapshot list

# Delete a specific snapshot or delete all
cli-ner snapshot delete <SNAPSHOT_ID>
cli-ner snapshot delete --all
```

---

### 10. `cli-ner diff` (or `cli-ner compare`) — Time-Travel Disk Growth Tracking

Track **what consumed space** between yesterday and today. Compares current live disk usage against previous snapshots to pinpoint which directories or files grew:

```bash
# Compare current live state with the most recent saved snapshot
cli-ner diff

# Compare two specific historical snapshots
cli-ner diff <SNAPSHOT_ID_1> <SNAPSHOT_ID_2>

# Filter to show only top growers and custom delta threshold
cli-ner diff --top 10 --min-delta 50MB

# Save a new snapshot after calculating diff
cli-ner diff --save

# Output structured JSON for automation or scripting
cli-ner diff --format json
```

---

### 11. `cli-ner maintain` — System Maintenance Actions

Runs macOS system commands that are not "delete a path": index rebuilds, disk checks, DNS flush.
Same UX as `clean`: **dry-run by default**, `--execute` to act, a confirmation prompt before
anything runs. Every command is a fixed program with fixed arguments, never a shell string.

```bash
# Preview every action, with risk, admin need, status and the exact commands
cli-ner maintain

# Run one category
cli-ner maintain --category system-state --allow-admin --execute

# Heavy rebuilds (Spotlight, locate): dedicated flag + one prompt each, even with --yes
cli-ner maintain --category system-index --include-heavy --allow-admin --execute
```

| Category | Action | Admin | Risk |
|---|---|---|---|
| `system-index` | Rebuild LaunchServices database ("Open With" duplicates) | | low |
| `system-index` | Vacuum Mail envelope index (Mail.app must be closed) | | low |
| `system-index` | Rebuild Spotlight index (`mdutil -E /`) — **opt-in** | ✓ | medium |
| `system-index` | Rebuild `locate` database — **opt-in** | ✓ | medium |
| `disk-maintenance` | SMART status of the boot disk (read-only) | | low |
| `disk-maintenance` | Verify APFS volume, no changes (`fsck_apfs -n`) | ✓ | low |
| `disk-maintenance` | Thin expired local Time Machine snapshots | ✓ | medium |
| `disk-maintenance` | Delete crash core dumps in `/cores` | ✓ | medium |
| `extended-cache` | Reset user font cache, reset Quick Look cache | | low |
| `extended-cache` | System service caches, print spooler cache | ✓ | low |
| `system-state` | Flush DNS cache | ✓ | low |
| `system-state` | Purge inactive memory (frees RAM, not disk) | ✓ | low |

- **`--allow-admin`**: without it, admin actions are listed but skipped. With it, macOS asks for
  your password once (`sudo -v`); CLI-NER never sees it.
- **`--include-heavy`**: Spotlight and `locate` rebuilds are never part of a normal run. Even with
  the flag, each one asks for its own yes/no, which `--yes` does not skip, so they never run
  unattended.
- Medium-risk actions need confirmation unless you pass `--yes`. Every run goes to the same audit
  log as `clean` (`cli-ner report`).

---

## 🛡️ Detailed Safety Model

### Three levels of risk, safest by default

| | What it does | How to trigger | Reversible? |
|---|---|---|---|
| **Preview** | Measures and shows, touches nothing | *default* | — |
| **Trash** | Moves to `~/.Trash` | `--execute` | ✅ until you empty it |
| **Permanent** | Actually deletes | `--execute --force` | ❌ **no** |

### Validation pipeline

Every candidate passes these checks before anything happens to it:

1. **Blocklist Check**: If the target belongs to critical system paths (`/System`, `/usr`, etc.) or personal user data (`~/.ssh`, `~/Documents`, `~/Desktop`, `~/Library/Mail`), the operation is **immediately rejected**.
2. **Allowlist Verification**: The path must strictly belong to an authorized cleanable category.
3. **Root Folder Protection**: Deleting root folders (e.g., `~/Library/Caches`) is prohibited; only their individual child items can be cleaned.
4. **Symlink Guard**: Symlinks are never followed — a symlink is moved or removed as a link, so a link pointing outside a target directory can never take its destination with it.
5. **Process Safety Check**: Sensitive operations (e.g., Xcode DerivedData) verify that the corresponding application is not currently active.

### Contextual protections

- **Running browsers**: cache folders belonging to a browser that is open right now are excluded automatically, so sessions, tabs and extension state cannot be corrupted. Close the browser and re-run to reclaim them too.
- **In-use temp files**: entries in `/private/tmp` and `/private/var/tmp` become eligible only after 24 hours of inactivity — scratch files, sockets and locks held by live processes are never pulled out from under them.
- **Emptying the Trash acts only on the pre-scanned list** and refuses anything that is not a direct child of `~/.Trash`. A `--category all` run therefore cannot permanently destroy the caches it had just moved there for safekeeping.
- **Docker gets its own confirmation** in execute mode, because data in containers that are not backed by a persistent volume is lost for good.
- **No privilege escalation**: CLI-NER never runs as root. Locations that need `sudo` are only reported by `bloat --system`, together with the command to run yourself.

### Restoring

Every trashed item's original path *and* its destination inside `~/.Trash` are recorded in the
audit log. `cli-ner report --last` shows both, so a restore is a plain `mv` even though a direct
POSIX rename into the Trash leaves no Finder "Put Back" metadata.

---

## 🧪 Running Tests

The project includes a unit and end-to-end integration test suite covering the safety
guarantees directly — that a dry-run touches nothing, that a trash move is reported as trashed
rather than freed and records its restore path, that targets outside the allowlist are refused,
that emptying the Trash rejects anything that is not a direct child of `~/.Trash`, and that
size measurement skips nothing and never follows symlinks.

```bash
# Run all tests
cargo test

# Run tests with detailed output
cargo test -- --nocapture

# Lints and formatting, as enforced by CI
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
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

