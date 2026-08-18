# Contributing to CLI-NER

Thank you for your interest in contributing to **CLI-NER**! We welcome bug fixes, documentation improvements, new cache target modules, and performance enhancements.

---

## 🧭 Guiding Philosophy: Safety First

`cli-ner` operates on users' file systems. Therefore, **safety is our absolute highest priority**:
1. **Never delete permanently**: Always route file removals through macOS Trash (`trash` crate).
2. **Strict blocklists**: Never touch protected system folders (`/System`, `/Library`, `/usr`, `/bin`) or private user directories (`~/Documents`, `~/Desktop`, `~/.ssh`, `~/Library/Mail`, `~/Library/Keychains`).
3. **Controlled allowlist**: Only clean explicitly whitelisted cache subdirectories (e.g. `~/Library/Caches/<app>/*`), never delete the root cache folder itself.
4. **Active process checking**: Always check if related applications (browsers, Xcode, Docker, etc.) are running before recommending or cleaning their caches.

---

## 🛠️ Development Setup

### Prerequisites
- macOS (Apple Silicon or Intel)
- Rust & Cargo (1.80+)

### Building and Running Locally
```bash
# Clone repository
git clone https://github.com/fabrizioriccardo73/cli-ner.git
cd cli-ner

# Check code
cargo check

# Run tests
cargo test

# Build debug binary
cargo build

# Run CLI commands directly
cargo run -- doctor
cargo run -- scan
cargo run -- clean
```

---

## 🧪 Testing and Quality Standards

Before submitting a Pull Request, ensure that all automated checks pass:

```bash
# 1. Format code
cargo fmt --all -- --check

# 2. Run Clippy lints
cargo clippy --all-targets -- -D warnings

# 3. Run all unit and integration tests
cargo test --all-targets
```

---

## 📦 Adding a New Cache Target

When proposing support for a new developer tool or application cache:
1. Place target definitions inside `src/cleaner/` or appropriate module.
2. Implement validation against `src/safety/validator.rs`.
3. Add a process check if the application could be corrupted while running.
4. Include unit tests in the module and an integration test in `tests/`.
5. Update `README.md` to document the new target.

---

## 🚀 Submitting a Pull Request

1. Fork the repository and create your branch from `master`:
   ```bash
   git checkout -b feature/my-cool-cache-target
   ```
2. Commit your changes with clear, descriptive commit messages.
3. Push to your fork and submit a Pull Request.
4. Fill in the Pull Request template and safety checklist.

Thank you for helping make macOS disk cleanup safer and faster for everyone!
