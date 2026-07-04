//! Interactive update-check policy and the self-update I/O shell.

// Nothing in `main` calls this module yet; until the update check is wired into
// `main`, every item is unreachable in the bin build. Remove this when it is.
#![allow(dead_code)]

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use self_update::backends::github;
use self_update::cargo_crate_version;
use self_update::version::bump_is_greater;

const DAY_SECS: u64 = 24 * 60 * 60;
const REPO_OWNER: &str = "graysonarts";
const REPO_NAME: &str = "prd-state";

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Check,
    Skip,
}

/// `now`/`last_check` are Unix epoch seconds; `None` means never checked.
#[must_use]
pub fn update_decision(disabled: bool, is_tty: bool, last_check: Option<u64>, now: u64) -> Decision {
    if disabled || !is_tty {
        return Decision::Skip;
    }
    match last_check {
        Some(t) if now.saturating_sub(t) < DAY_SECS => Decision::Skip,
        _ => Decision::Check,
    }
}

pub fn self_update() -> Result<String> {
    let status = github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(REPO_NAME)
        .current_version(cargo_crate_version!())
        .show_download_progress(std::io::stdout().is_terminal())
        .build()?
        .update()?;
    Ok(if status.updated() {
        format!("updated to {}", status.version())
    } else {
        format!("already up to date ({})", status.version())
    })
}

/// Best-effort: any failure (no `HOME`, cache or network error) is swallowed, never surfaced.
pub fn check() {
    let Some(cache) = cache_file() else { return };
    let disabled = std::env::var_os("PRD_STATE_NO_UPDATE").is_some();
    let now = unix_now();
    if update_decision(disabled, std::io::stdout().is_terminal(), read_stamp(&cache), now)
        == Decision::Skip
    {
        return;
    }
    write_stamp(&cache, now);
    if let Some(latest) = latest_version()
        && bump_is_greater(cargo_crate_version!(), &latest).unwrap_or(false)
    {
        eprintln!("prd-state {latest} available — run: prd-state self-update");
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn cache_file() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("HOME")?).join(".cache/prd-state/last_update_check"))
}

fn read_stamp(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn write_stamp(path: &Path, now: u64) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, now.to_string());
}

fn latest_version() -> Option<String> {
    let releases = github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()
        .ok()?
        .fetch()
        .ok()?;
    Some(releases.into_iter().next()?.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000_000;

    #[test]
    fn disabled_skips() {
        assert_eq!(update_decision(true, true, None, NOW), Decision::Skip);
    }

    #[test]
    fn non_tty_skips() {
        assert_eq!(update_decision(false, false, None, NOW), Decision::Skip);
    }

    #[test]
    fn recent_check_skips() {
        assert_eq!(update_decision(false, true, Some(NOW - 3600), NOW), Decision::Skip);
    }

    #[test]
    fn stale_check_checks() {
        assert_eq!(update_decision(false, true, Some(NOW - 25 * 3600), NOW), Decision::Check);
    }

    #[test]
    fn never_checked_checks() {
        assert_eq!(update_decision(false, true, None, NOW), Decision::Check);
    }
}
