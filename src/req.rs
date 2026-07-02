//! Manual requirement-registry edits — the fallback when `sync` cannot parse.

use crate::state::{self, ReqType};
use anyhow::Result;
use std::path::Path;

pub fn add(dir: &Path, id: &str, req_type: ReqType, text: &str) -> Result<String> {
    let mut st = state::load(dir)?;
    st.requirements.add(id, req_type, text)?;
    state::save(dir, &st)?;
    Ok(format!("added {id} ({req_type:?})"))
}

pub fn remove(dir: &Path, id: &str) -> Result<String> {
    let mut st = state::load(dir)?;
    let msg = match st.requirements.remove(id)? {
        // Milestones stay in the registry as history; the PRD may still list them.
        ReqType::Milestone => format!("marked {id} removed"),
        ReqType::Invariant => format!("deleted invariant {id}"),
    };
    state::save(dir, &st)?;
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ReqStatus, State};
    use tempfile::TempDir;

    fn dir_with_state() -> TempDir {
        let dir = TempDir::new().unwrap();
        state::save(dir.path(), &State::new("PRD-test.md")).unwrap();
        dir
    }

    #[test]
    fn add_milestone_is_active() {
        let dir = dir_with_state();
        add(dir.path(), "ISC-X1", ReqType::Milestone, "does a thing").unwrap();
        let st = state::load(dir.path()).unwrap();
        assert_eq!(st.requirements[0].id, "ISC-X1");
        assert_eq!(st.requirements[0].status, Some(ReqStatus::Active));
        assert_eq!(st.requirements[0].text, "does a thing");
    }

    #[test]
    fn add_invariant_has_no_status() {
        let dir = dir_with_state();
        add(dir.path(), "INV-A1", ReqType::Invariant, "always").unwrap();
        let st = state::load(dir.path()).unwrap();
        assert_eq!(st.requirements[0].status, None);
    }

    #[test]
    fn add_duplicate_rejected() {
        let dir = dir_with_state();
        add(dir.path(), "ISC-X1", ReqType::Milestone, "a").unwrap();
        let err = add(dir.path(), "ISC-X1", ReqType::Milestone, "b").unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }

    #[test]
    fn remove_milestone_marks_removed_keeps_entry() {
        let dir = dir_with_state();
        add(dir.path(), "ISC-X1", ReqType::Milestone, "a").unwrap();
        let msg = remove(dir.path(), "ISC-X1").unwrap();
        assert!(msg.contains("marked ISC-X1 removed"));
        let st = state::load(dir.path()).unwrap();
        assert_eq!(st.requirements.len(), 1);
        assert_eq!(st.requirements[0].status, Some(ReqStatus::Removed));
    }

    #[test]
    fn remove_invariant_deletes_entry() {
        let dir = dir_with_state();
        add(dir.path(), "INV-A1", ReqType::Invariant, "a").unwrap();
        remove(dir.path(), "INV-A1").unwrap();
        assert!(state::load(dir.path()).unwrap().requirements.is_empty());
    }

    #[test]
    fn remove_unknown_errors() {
        let dir = dir_with_state();
        let err = remove(dir.path(), "ISC-NOPE").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
