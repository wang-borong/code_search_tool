use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::Result;

pub(crate) fn user_data_dir() -> Result<PathBuf> {
    let preferred = dirs::data_local_dir().map(|path| path.join("fcs"));
    let fallback = std::env::current_dir()?.join(".fcs-data");
    writable_dir_or_fallback(preferred.as_deref(), &fallback)
}

pub(crate) fn workspace_cache_root(root: &Path) -> Result<PathBuf> {
    let preferred = dirs::cache_dir().map(|path| path.join("fcs").join("workspaces"));
    let fallback = root.join(".fcs-cache").join("workspaces");
    writable_dir_or_fallback(preferred.as_deref(), &fallback)
}

fn writable_dir_or_fallback(preferred: Option<&Path>, fallback: &Path) -> Result<PathBuf> {
    if let Some(preferred) = preferred {
        if ensure_writable_dir(preferred).is_ok() {
            return Ok(preferred.to_path_buf());
        }
    }

    ensure_writable_dir(fallback)?;
    Ok(fallback.to_path_buf())
}

fn ensure_writable_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    let probe = path.join(format!(".fcs-write-test-{}-{}", std::process::id(), now_nanos()));
    OpenOptions::new().write(true).create_new(true).open(&probe)?;
    fs::remove_file(probe)?;
    Ok(())
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_a_writable_fallback_when_preferred_is_missing() {
        let root = std::env::temp_dir().join(format!("fcs_cache_test_{}", std::process::id()));
        let fallback = root.join("fallback");
        let _ = fs::remove_dir_all(&root);

        let selected = writable_dir_or_fallback(None, &fallback).unwrap();

        assert_eq!(selected, fallback);
        assert!(selected.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
