//! Manual requirement-registry edits — the fallback when `sync` cannot parse.

use crate::state::{ReqType, State};
#[cfg(test)]
use crate::state; // test module reaches state::load/save through this alias
use anyhow::Result;
use std::path::Path;

pub fn add(
    dir: &Path,
    id: &str,
    req_type: ReqType,
    text: &str,
    annotation: Option<&str>,
) -> Result<String> {
    State::update(dir, |st| {
        st.requirements.add(id, req_type, text, annotation)?;
        Ok(format!("added {id} ({req_type:?})"))
    })
}

pub fn remove(dir: &Path, id: &str) -> Result<String> {
    State::update(dir, |st| {
        match st.requirements.remove(id)? {
            // Milestones stay in the registry as history; the PRD may still list them.
            ReqType::Milestone => Ok(format!("marked {id} removed")),
            ReqType::Invariant => Ok(format!("deleted invariant {id}")),
        }
    })
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
        add(dir.path(), "ISC-X1", ReqType::Milestone, "does a thing", None).unwrap();
        let st = state::load(dir.path()).unwrap();
        assert_eq!(st.requirements[0].id, "ISC-X1");
        assert_eq!(st.requirements[0].status, Some(ReqStatus::Active));
        assert_eq!(st.requirements[0].text, "does a thing");
    }

    #[test]
    fn add_stores_annotation_and_defaults_to_absent() {
        let dir = dir_with_state();
        add(dir.path(), "INV-A1", ReqType::Invariant, "always", Some("blocking")).unwrap();
        add(dir.path(), "ISC-X1", ReqType::Milestone, "a thing", None).unwrap();
        let st = state::load(dir.path()).unwrap();
        assert_eq!(st.requirements[0].annotation.as_deref(), Some("blocking"));
        assert_eq!(st.requirements[1].annotation, None);
    }

    #[test]
    fn add_invariant_has_no_status() {
        let dir = dir_with_state();
        add(dir.path(), "INV-A1", ReqType::Invariant, "always", None).unwrap();
        let st = state::load(dir.path()).unwrap();
        assert_eq!(st.requirements[0].status, None);
    }

    #[test]
    fn add_duplicate_rejected() {
        let dir = dir_with_state();
        add(dir.path(), "ISC-X1", ReqType::Milestone, "a", None).unwrap();
        let err = add(dir.path(), "ISC-X1", ReqType::Milestone, "b", None).unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }

    #[test]
    fn remove_milestone_marks_removed_keeps_entry() {
        let dir = dir_with_state();
        add(dir.path(), "ISC-X1", ReqType::Milestone, "a", None).unwrap();
        let msg = remove(dir.path(), "ISC-X1").unwrap();
        assert!(msg.contains("marked ISC-X1 removed"));
        let st = state::load(dir.path()).unwrap();
        assert_eq!(st.requirements.len(), 1);
        assert_eq!(st.requirements[0].status, Some(ReqStatus::Removed));
    }

    #[test]
    fn remove_invariant_deletes_entry() {
        let dir = dir_with_state();
        add(dir.path(), "INV-A1", ReqType::Invariant, "a", None).unwrap();
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
