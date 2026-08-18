use crate::utils::fs::expand_tilde;
use std::path::Path;

/// List of absolute/relative path patterns that are strictly forbidden from being modified or deleted.
pub const SYSTEM_BLOCKLIST: &[&str] = &[
    "/System",
    "/usr",
    "/bin",
    "/sbin",
    "/private/var/db",
    "/Library/System",
    "/Library/Apple",
    "/Library/Preferences/SystemConfiguration",
    "/etc",
    "/dev",
    "/Volumes",
];

/// List of user data paths that are strictly protected against any modification or deletion.
pub const USER_BLOCKLIST: &[&str] = &[
    "~/.ssh",
    "~/.gnupg",
    "~/.config",
    "~/.gitconfig",
    "~/.zshrc",
    "~/.bashrc",
    "~/.bash_profile",
    "~/.profile",
    "~/Documents",
    "~/Desktop",
    "~/Pictures",
    "~/Music",
    "~/Movies",
    "~/Library/Keychains",
    "~/Library/Mail",
    "~/Library/Messages",
    "~/Library/Calendars",
    "~/Library/Contacts",
    "~/Library/Safari",
    "~/Library/Application Support/MobileSync",
    "~/Library/Photos",
    "~/Library/Containers/com.apple.mail",
    "~/Library/Containers/com.apple.iChat",
];

/// Check if a given path falls within the forbidden Blocklist.
pub fn is_blocked<P: AsRef<Path>>(path: P) -> (bool, Option<String>) {
    let path = path.as_ref();
    let abs_path = if path.is_relative() {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };

    // 1. Check system blocklist
    for &blocked in SYSTEM_BLOCKLIST {
        let blocked_path = Path::new(blocked);
        if abs_path == blocked_path || abs_path.starts_with(blocked_path) {
            return (true, Some(format!("Protected system path: {}", blocked)));
        }
        if blocked_path.starts_with(&abs_path) && abs_path != Path::new("/") {
            return (
                true,
                Some(format!("Parent of protected system path: {}", blocked)),
            );
        }
    }

    // 2. Check user blocklist (expanded with user home)
    for &blocked in USER_BLOCKLIST {
        let expanded = expand_tilde(blocked);
        if abs_path == expanded || abs_path.starts_with(&expanded) {
            return (true, Some(format!("Protected user data path: {}", blocked)));
        }
        if expanded.starts_with(&abs_path)
            && abs_path != dirs::home_dir().unwrap_or_default()
            && abs_path != Path::new("/")
        {
            return (
                true,
                Some(format!("Parent of protected user path: {}", blocked)),
            );
        }
    }

    // 3. Strict protection against root and home dir direct deletion
    if abs_path == Path::new("/") {
        return (true, Some("Cannot target root filesystem /".into()));
    }
    if let Some(home) = dirs::home_dir() {
        if abs_path == home {
            return (true, Some("Cannot target user home directory root".into()));
        }
    }

    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_blocklist() {
        let (blocked, reason) = is_blocked("/System/Library");
        assert!(blocked);
        assert!(reason.is_some());

        let (blocked, _) = is_blocked("/usr/bin/git");
        assert!(blocked);

        let (blocked, _) = is_blocked("/bin");
        assert!(blocked);
    }

    #[test]
    fn test_user_blocklist() {
        if let Some(home) = dirs::home_dir() {
            let ssh_path = home.join(".ssh/id_rsa");
            let (blocked, reason) = is_blocked(&ssh_path);
            assert!(blocked);
            assert!(reason.is_some());

            let docs_path = home.join("Documents/Project/file.txt");
            let (blocked, _) = is_blocked(&docs_path);
            assert!(blocked);

            let mail_path = home.join("Library/Mail/V10");
            let (blocked, _) = is_blocked(&mail_path);
            assert!(blocked);
        }
    }
}
