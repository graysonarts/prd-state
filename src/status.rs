//! Resume-aware status summary, computed from state (never asserted by the agent).

use crate::state::{Phase, State, SubgoalStatus};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct Summary {
    pub iteration: u32,
    pub phase: Option<Phase>,
    pub next_subgoal: Option<NextSubgoal>,
    pub pending_milestones: usize,
    pub resume: Phase,
}

#[derive(Debug, Serialize)]
pub struct NextSubgoal {
    pub id: String,
    pub description: String,
}

/// Compute the summary. `artifact_root` is the base for resolving
/// `current_action.artifacts` when applying the ACT→VERIFY shortcut.
pub fn summary(state: &State, artifact_root: &Path) -> Summary {
    let next_subgoal = state
        .subgoals
        .iter()
        .find(|sg| sg.status != SubgoalStatus::Complete)
        .map(|sg| NextSubgoal {
            id: sg.id.clone(),
            description: sg.description.clone(),
        });
    let pending_milestones = state.requirements.pending_milestones();
    Summary {
        iteration: state.iteration,
        phase: state.current_phase,
        next_subgoal,
        pending_milestones,
        resume: resume_phase(state, artifact_root),
    }
}

/// The `prd_work_loop` resume table.
fn resume_phase(state: &State, artifact_root: &Path) -> Phase {
    match state.current_phase {
        None => Phase::Observe,
        Some(Phase::Decide) => {
            if state.pre_flight_checklist.is_empty() {
                Phase::Decide
            } else {
                Phase::Act
            }
        }
        Some(Phase::Act) => {
            let all_exist = state.current_action.as_ref().is_some_and(|a| {
                !a.artifacts.is_empty()
                    && a.artifacts.iter().all(|f| artifact_root.join(f).exists())
            });
            if all_exist {
                Phase::Verify
            } else {
                Phase::Act
            }
        }
        Some(p) => p,
    }
}

pub fn render(s: &Summary) -> String {
    let phase = s
        .phase.map_or_else(|| "none (between iterations)".into(), |p| p.to_string());
    let subgoal = s
        .next_subgoal
        .as_ref().map_or_else(|| "none (all complete)".into(), |sg| format!("{} — {}", sg.id, sg.description));
    format!(
        "iteration: {}\nphase: {}\nnext subgoal: {}\npending milestones: {}\nresume: {}",
        s.iteration, phase, subgoal, s.pending_milestones, s.resume
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ChecklistItem, CurrentAction, ReqType, Subgoal, Tier};
    use std::fs;
    use tempfile::TempDir;

    fn base_state() -> State {
        let mut s = State::new("PRD-test.md");
        s.iteration = 2;
        s.subgoals.push(Subgoal {
            id: "SG-1".into(),
            artifacts: vec!["src/a.rs".into()],
            tier: Tier::Standard,
            description: "first".into(),
            milestones: vec!["ISC-A1".into()],
            status: SubgoalStatus::Complete,
        });
        s.subgoals.push(Subgoal {
            id: "SG-2".into(),
            artifacts: vec!["src/b.rs".into()],
            tier: Tier::Standard,
            description: "second".into(),
            milestones: vec!["ISC-B1".into()],
            status: SubgoalStatus::Pending,
        });
        s.requirements.add("ISC-A1", ReqType::Milestone, "done").unwrap();
        s.requirements.mark_satisfied("ISC-A1").unwrap();
        s.requirements.add("ISC-B1", ReqType::Milestone, "todo").unwrap();
        s
    }

    #[test]
    fn summary_reports_iteration_phase_subgoal_pending() {
        let state = base_state();
        let s = summary(&state, Path::new("."));
        assert_eq!(s.iteration, 2);
        assert_eq!(s.phase, None);
        assert_eq!(s.next_subgoal.as_ref().unwrap().id, "SG-2");
        assert_eq!(s.pending_milestones, 1);
        let text = render(&s);
        assert!(text.contains("iteration: 2"), "{text}");
        assert!(text.contains("SG-2 — second"), "{text}");
        assert!(text.contains("pending milestones: 1"), "{text}");
        assert!(text.contains("resume: OBSERVE"), "{text}");
    }

    #[test]
    fn resume_null_and_repeatable_phases() {
        let mut state = base_state();
        let root = Path::new(".");
        assert_eq!(resume_phase(&state, root), Phase::Observe);
        for (set, expect) in [
            (Phase::Observe, Phase::Observe),
            (Phase::Orient, Phase::Orient),
            (Phase::Verify, Phase::Verify),
            (Phase::Update, Phase::Update),
        ] {
            state.current_phase = Some(set);
            assert_eq!(resume_phase(&state, root), expect);
        }
    }

    #[test]
    fn resume_decide_skips_to_act_when_preflight_present() {
        let mut state = base_state();
        state.current_phase = Some(Phase::Decide);
        assert_eq!(resume_phase(&state, Path::new(".")), Phase::Decide);
        state.pre_flight_checklist.push(ChecklistItem {
            id: "ISC-B1".into(),
            req_type: ReqType::Milestone,
        });
        assert_eq!(resume_phase(&state, Path::new(".")), Phase::Act);
    }

    #[test]
    fn resume_act_skips_to_verify_when_artifacts_exist() {
        let dir = TempDir::new().unwrap();
        let mut state = base_state();
        state.current_phase = Some(Phase::Act);
        state.current_action = Some(CurrentAction {
            artifacts: vec!["out.rs".into(), "missing.rs".into()],
            tier: Tier::Standard,
            description: "x".into(),
            applicable_milestones: vec!["ISC-B1".into()],
        });
        fs::write(dir.path().join("out.rs"), "").unwrap();
        // one artifact missing -> re-run ACT
        assert_eq!(resume_phase(&state, dir.path()), Phase::Act);
        fs::write(dir.path().join("missing.rs"), "").unwrap();
        assert_eq!(resume_phase(&state, dir.path()), Phase::Verify);
    }

    #[test]
    fn resume_act_without_current_action_reruns_act() {
        let mut state = base_state();
        state.current_phase = Some(Phase::Act);
        assert_eq!(resume_phase(&state, Path::new(".")), Phase::Act);
    }

    #[test]
    fn json_output_round_trips() {
        let state = base_state();
        let s = summary(&state, Path::new("."));
        let json = serde_json::to_string(&s).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["iteration"], 2);
        assert_eq!(v["resume"], "OBSERVE");
        assert_eq!(v["next_subgoal"]["id"], "SG-2");
        assert_eq!(v["pending_milestones"], 1);
    }
}
