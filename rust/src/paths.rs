//! Deterministic, machine-independent discovery of the app's asset roots.
//!
//! Nothing in the codebase may hardcode an absolute machine path (`/mnt/...`,
//! `/home/<user>/...`). Resolution happens in this order:
//! 1. the working directory and its parent (running from a source checkout),
//! 2. relative to the executable (packaged releases: `<exe dir>/assets`),
//! 3. the user-local root `~/.remoainter/assets` (imports, user libraries).

use std::path::PathBuf;

/// Candidate `assets/` roots, in probe order.
pub fn assets_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("assets"), PathBuf::from("../assets")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("assets"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("assets"));
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            candidates.push(PathBuf::from(home).join(".remoainter").join("assets"));
        }
    }
    candidates
}

/// First `assets/` root that exists, if any.
pub fn find_assets_dir() -> Option<PathBuf> {
    assets_dir_candidates().into_iter().find(|p| p.is_dir())
}

/// First `assets/<sub-path>` that exists.
pub fn find_assets_subdir(sub: &str) -> Option<PathBuf> {
    assets_dir_candidates()
        .into_iter()
        .map(|p| p.join(sub))
        .find(|p| p.is_dir())
}

/// User default export location: `<home>/REMoAINTER exports` (created lazily by
/// the export pipeline on first use).
pub fn default_export_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Documents").join("REMoAINTER"))
        .unwrap_or_else(|| PathBuf::from("exports"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_are_relative_first_and_never_absolute_machine_paths() {
        let cands = assets_dir_candidates();
        assert!(!cands.is_empty());
        assert!(!cands[0].is_absolute());
        assert!(!cands[1].is_absolute());
        for p in &cands[..2] {
            let s = p.to_string_lossy();
            assert!(!s.starts_with("/mnt/"), "machine path leaked: {s}");
            assert!(!s.starts_with("/home/"), "machine path leaked: {s}");
        }
    }

    #[test]
    fn default_export_dir_is_not_a_repo_path() {
        let d = default_export_dir();
        assert!(!d.to_string_lossy().contains("mnt/shared"));
    }
}
