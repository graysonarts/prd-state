//! DECIDE: lock a subgoal in as the iteration's work and derive its pre-flight.

use crate::state::{self, ChecklistItem, CurrentAction, ReqType, SubgoalStatus};
use anyhow::{bail, Result};
use std::path::Path;

/// Mark the subgoal in_progress, set current_action from it, derive the
/// pre-flight (every registered invariant + exactly the subgoal's milestones),
/// and return the printed checklist.
pub fn run(dir: &Path, sg_id: &str) -> Result<String> {
    let mut st = state::load(dir)?;
    let sg = match st.subgoals.iter_mut().find(|sg| sg.id == sg_id) {
        Some(sg) => sg,
        None => bail!("subgoal {sg_id} not found"),
    };
    if sg.status == SubgoalStatus::Complete {
        bail!("subgoal {sg_id} is already complete");
    }
    sg.status = SubgoalStatus::InProgress;
    let action = CurrentAction {
        artifacts: sg.artifacts.clone(),
        tier: sg.tier,
        description: sg.description.clone(),
        applicable_milestones: sg.milestones.clone(),
    };
    let milestones = sg.milestones.clone();

    let mut checklist: Vec<ChecklistItem> = st
        .requirements
        .iter()
        .filter(|r| r.req_type == ReqType::Invariant)
        .map(|r| ChecklistItem { id: r.id.clone(), req_type: ReqType::Invariant })
        .collect();
    for m in &milestones {
        if !st.requirements.iter().any(|r| r.id == *m) {
            bail!("milestone {m} not in registry; run `sync` or `req add` first");
        }
        checklist.push(ChecklistItem { id: m.clone(), req_type: ReqType::Milestone });
    }

    let mut out = format!("PRE-FLIGHT for {}", action.artifacts.join(", "));
    let width = checklist.iter().map(|c| c.id.len()).max().unwrap_or(0);
    for item in &checklist {
        let text = &st.requirements.iter().find(|r| r.id == item.id).unwrap().text;
        out.push_str(&format!("\n  {:width$}  {}", item.id, text));
    }

    st.current_action = Some(action);
    st.pre_flight_checklist = checklist;
    state::save(dir, &st)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Requirement, ReqStatus, State, Subgoal, Tier};
    use tempfile::TempDir;

    fn req(id: &str, req_type: ReqType) -> Requirement {
        Requirement {
            id: id.into(),
            req_type,
            status: matches!(req_type, ReqType::Milestone).then_some(ReqStatus::Active),
            text: format!("text for {id}"),
        }
    }

    fn dir_with_subgoal() -> TempDir {
        let dir = TempDir::new().unwrap();
        let mut st = State::new("PRD-test.md");
        st.requirements = vec![
            req("INV-A1", ReqType::Invariant),
            req("ISC-X1", ReqType::Milestone),
            req("ISC-X2", ReqType::Milestone),
            req("ISC-Y1", ReqType::Milestone), // registered but not in the subgoal
        ];
        st.subgoals.push(Subgoal {
            id: "SG-1".into(),
            artifacts: vec!["src/x.rs".into()],
            tier: Tier::Standard,
            description: "module x".into(),
            milestones: vec!["ISC-X1".into(), "ISC-X2".into()],
            status: SubgoalStatus::Pending,
        });
        state::save(dir.path(), &st).unwrap();
        dir
    }

    #[test]
    fn decide_sets_in_progress_action_and_preflight() {
        let dir = dir_with_subgoal();
        run(dir.path(), "SG-1").unwrap();
        let st = state::load(dir.path()).unwrap();
        assert_eq!(st.subgoals[0].status, SubgoalStatus::InProgress);
        let action = st.current_action.unwrap();
        assert_eq!(action.artifacts, vec!["src/x.rs"]);
        assert_eq!(action.tier, Tier::Standard);
        assert_eq!(action.description, "module x");
        assert_eq!(action.applicable_milestones, vec!["ISC-X1", "ISC-X2"]);
        let ids: Vec<&str> = st.pre_flight_checklist.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["INV-A1", "ISC-X1", "ISC-X2"]); // all invariants + exactly the subgoal's milestones
    }

    #[test]
    fn decide_prints_each_checklist_item() {
        let dir = dir_with_subgoal();
        let out = run(dir.path(), "SG-1").unwrap();
        assert!(out.contains("PRE-FLIGHT for src/x.rs"));
        for id in ["INV-A1", "ISC-X1", "ISC-X2"] {
            assert!(out.contains(id), "missing {id} in: {out}");
        }
        assert!(out.contains("text for ISC-X1"));
        assert!(!out.contains("ISC-Y1"));
    }

    #[test]
    fn decide_unknown_subgoal_errors() {
        let dir = dir_with_subgoal();
        let err = run(dir.path(), "SG-9").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn decide_complete_subgoal_errors() {
        let dir = dir_with_subgoal();
        let mut st = state::load(dir.path()).unwrap();
        st.subgoals[0].status = SubgoalStatus::Complete;
        state::save(dir.path(), &st).unwrap();
        let err = run(dir.path(), "SG-1").unwrap_err();
        assert!(err.to_string().contains("already complete"));
    }

    #[test]
    fn decide_unregistered_milestone_errors() {
        let dir = dir_with_subgoal();
        let mut st = state::load(dir.path()).unwrap();
        st.subgoals[0].milestones.push("ISC-Z9".into());
        state::save(dir.path(), &st).unwrap();
        let err = run(dir.path(), "SG-1").unwrap_err();
        assert!(err.to_string().contains("ISC-Z9 not in registry"));
    }
}
