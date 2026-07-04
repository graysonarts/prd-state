//! Subgoal list edits. Split/merge composes from add + remove.

use crate::state::{State, Subgoal, SubgoalStatus, Tier};
#[cfg(test)]
use crate::state; // test module reaches state::load/save through this alias
use anyhow::{bail, Result};
use std::path::Path;

// The six fields map 1:1 onto Subgoal; a params struct would only move the list.
#[allow(clippy::too_many_arguments)]
pub fn add(
    dir: &Path,
    id: &str,
    tier: Tier,
    artifacts: Vec<String>,
    milestones: Vec<String>,
    description: &str,
) -> Result<String> {
    if artifacts.is_empty() {
        bail!("subgoal {id} needs at least one artifact");
    }
    State::update(dir, |st| {
        if st.subgoals.iter().any(|sg| sg.id == id) {
            bail!("subgoal {id} already exists");
        }
        st.subgoals.push(Subgoal {
            id: id.to_string(),
            artifacts,
            tier,
            description: description.to_string(),
            milestones,
            status: SubgoalStatus::Pending,
        });
        Ok(format!("added {id} (pending)"))
    })
}

pub fn remove(dir: &Path, id: &str) -> Result<String> {
    State::update(dir, |st| {
        let before = st.subgoals.len();
        st.subgoals.retain(|sg| sg.id != id);
        if st.subgoals.len() == before {
            bail!("subgoal {id} not found");
        }
        Ok(format!("removed {id}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use tempfile::TempDir;

    fn dir_with_state() -> TempDir {
        let dir = TempDir::new().unwrap();
        state::save(dir.path(), &State::new("PRD-test.md")).unwrap();
        dir
    }

    #[test]
    fn add_writes_all_fields_pending() {
        let dir = dir_with_state();
        add(
            dir.path(),
            "SG-1",
            Tier::Standard,
            vec!["src/a.rs".into(), "src/a_test.rs".into()],
            vec!["ISC-A1".into(), "ISC-A2".into()],
            "module a",
        )
        .unwrap();
        let sg = &state::load(dir.path()).unwrap().subgoals[0];
        assert_eq!(sg.id, "SG-1");
        assert_eq!(sg.tier, Tier::Standard);
        assert_eq!(sg.artifacts, vec!["src/a.rs", "src/a_test.rs"]);
        assert_eq!(sg.milestones, vec!["ISC-A1", "ISC-A2"]);
        assert_eq!(sg.description, "module a");
        assert_eq!(sg.status, SubgoalStatus::Pending);
    }

    #[test]
    fn add_duplicate_or_empty_artifacts_rejected() {
        let dir = dir_with_state();
        add(dir.path(), "SG-1", Tier::Trivial, vec!["a".into()], vec![], "x").unwrap();
        let dup = add(dir.path(), "SG-1", Tier::Trivial, vec!["b".into()], vec![], "y").unwrap_err();
        assert!(dup.to_string().contains("already exists"));
        let empty = add(dir.path(), "SG-2", Tier::Trivial, vec![], vec![], "z").unwrap_err();
        assert!(empty.to_string().contains("at least one artifact"));
    }

    #[test]
    fn remove_deletes_subgoal() {
        let dir = dir_with_state();
        add(dir.path(), "SG-1", Tier::Trivial, vec!["a".into()], vec![], "x").unwrap();
        remove(dir.path(), "SG-1").unwrap();
        assert!(state::load(dir.path()).unwrap().subgoals.is_empty());
    }

    #[test]
    fn remove_unknown_errors() {
        let dir = dir_with_state();
        let err = remove(dir.path(), "SG-9").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
