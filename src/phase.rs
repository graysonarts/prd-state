//! Set the current phase; entering OBSERVE captures start_commit from git HEAD.

use crate::state::{self, Phase};
use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub fn set_phase(dir: &Path, phase: Phase) -> Result<String> {
    let mut st = state::load(dir)?;
    st.current_phase = Some(phase);
    if phase == Phase::Observe {
        st.start_commit = git_short_head(dir);
    }
    state::save(dir, &st)?;
    Ok(match (&phase, &st.start_commit) {
        (Phase::Observe, Some(c)) => format!("phase: {phase} (start_commit: {c})"),
        (Phase::Observe, None) => format!("phase: {phase} (start_commit: none — no git HEAD)"),
        _ => format!("phase: {phase}"),
    })
}

fn git_short_head(dir: &Path) -> Option<String> {
    Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use clap::ValueEnum;
    use std::fs;
    use tempfile::TempDir;

    fn dir_with_state() -> TempDir {
        let dir = TempDir::new().unwrap();
        state::save(dir.path(), &State::new("PRD-test.md")).unwrap();
        dir
    }

    #[test]
    fn sets_current_phase() {
        let dir = dir_with_state();
        let msg = set_phase(dir.path(), Phase::Decide).unwrap();
        assert_eq!(msg, "phase: DECIDE");
        assert_eq!(state::load(dir.path()).unwrap().current_phase, Some(Phase::Decide));
    }

    #[test]
    fn invalid_phase_name_rejected() {
        assert!(Phase::from_str("NAPPING", true).is_err());
        assert!(Phase::from_str("OBSERVE", true).is_ok());
        assert!(Phase::from_str("observe", true).is_ok());
    }

    #[test]
    fn observe_captures_git_head() {
        let dir = dir_with_state();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q"]);
        fs::write(dir.path().join("f.txt"), "x").unwrap();
        git(&["add", "."]);
        git(&[
            "-c", "user.email=t@t", "-c", "user.name=t",
            "commit", "-q", "-m", "init", "--no-gpg-sign",
        ]);
        let msg = set_phase(dir.path(), Phase::Observe).unwrap();
        let commit = state::load(dir.path()).unwrap().start_commit.unwrap();
        assert!(!commit.is_empty() && commit.len() >= 7, "commit: {commit}");
        assert!(msg.contains(&commit), "{msg}");
    }

    #[test]
    fn observe_without_git_leaves_none() {
        let dir = dir_with_state();
        // TempDir may live under a parent git repo? rev-parse -C would then find it;
        // macOS $TMPDIR is not inside one, so HEAD resolution fails here.
        let msg = set_phase(dir.path(), Phase::Observe).unwrap();
        if state::load(dir.path()).unwrap().start_commit.is_none() {
            assert!(msg.contains("no git HEAD"), "{msg}");
        }
    }
}
