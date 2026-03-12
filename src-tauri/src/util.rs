//! Shared utility functions used across multiple modules.

use std::io;
use std::path::Path;

/// Replaces `~` at the beginning of a path with the actual home directory.
///
/// Uses `dirs::home_dir()` which is more robust than `std::env::var("HOME")`
/// as it also works when the HOME variable is not set.
///
/// # Examples
/// ```
/// // "~/Games/star-citizen" -> "/home/user/Games/star-citizen"
/// // "/absolute/path" -> "/absolute/path" (unchanged)
/// ```
pub(crate) fn expand_tilde(p: &str) -> String {
    if p.starts_with('~') {
        if let Some(h) = dirs::home_dir() {
            return p.replacen('~', &h.to_string_lossy(), 1);
        }
    }
    p.to_string()
}

/// Safely unpacks a tar archive, validating that no entry escapes the target directory.
///
/// This prevents path-traversal attacks where a malicious archive could contain
/// entries like `../../.bashrc` to overwrite files outside the intended directory.
pub(crate) fn safe_unpack<R: io::Read>(archive: &mut tar::Archive<R>, dst: &Path) -> io::Result<()> {
    let canonical_dst = dst.canonicalize()?;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        // Resolve the full target path
        let target = canonical_dst.join(&path);

        // Canonicalize parent to resolve any `..` components.
        // The file itself may not exist yet, so we canonicalize the parent.
        let parent = target.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Entry has no parent directory")
        })?;

        // Create parent directories so canonicalize can work
        std::fs::create_dir_all(parent)?;

        let canonical_target = parent.canonicalize()?.join(
            target.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Entry has no file name")
            })?
        );

        if !canonical_target.starts_with(&canonical_dst) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Path traversal detected: {}", path.display()),
            ));
        }

        entry.unpack(&canonical_target)?;
    }

    Ok(())
}

/// Validates a custom environment variable key.
///
/// Returns `Ok(())` if the key is valid, or an error message if not.
/// - Keys must only contain `[A-Za-z0-9_]`
/// - Certain security-sensitive keys are blocked to prevent abuse
pub(crate) fn validate_env_var_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("Environment variable key cannot be empty".to_string());
    }

    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!(
            "Environment variable key '{}' contains invalid characters (only A-Z, a-z, 0-9, _ allowed)",
            key
        ));
    }

    const BLOCKED_KEYS: &[&str] = &[
        "PATH", "LD_PRELOAD", "LD_LIBRARY_PATH", "HOME", "USER", "SHELL",
    ];

    if BLOCKED_KEYS.contains(&key) {
        return Err(format!(
            "Environment variable '{}' is blocked for security reasons",
            key
        ));
    }

    Ok(())
}
