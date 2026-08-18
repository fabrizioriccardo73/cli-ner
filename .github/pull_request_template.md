## Description
Briefly describe the purpose of this Pull Request and what changes or additions were made.

Closes # (issue number if applicable)

## Type of Change
- [ ] 🐛 Bug fix (non-breaking change which fixes an issue)
- [ ] ✨ New feature / Cache target (non-breaking change adding functionality)
- [ ] ⚡ Performance improvement
- [ ] 📝 Documentation update
- [ ] 🔧 Refactoring / Code quality

## 🛡️ Safety & Quality Checklist
- [ ] **Strict Safety**: Verified that no system blocklist (`/System`, `/usr`, `/bin`, etc.) or user data directories (`~/Documents`, `~/.ssh`, etc.) are targeted.
- [ ] **Reversibility**: Files are moved to Trash (`~/.Trash`) rather than permanently deleted.
- [ ] **Process Check**: If cleaning tool/browser caches, verified running process checks are implemented.
- [ ] **Tests Added / Updated**: Added unit and/or integration tests for new functionality.
- [ ] **Local Verification**: Ran `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` locally.
