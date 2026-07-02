//! UPDATE: close out an iteration — registry bookkeeping, subgoal status,
//! stall handling, PRD markdown writes, and transient-field reset.

use crate::prd_md;
use crate::state::{self, ReqStatus, ReqType, SubgoalStatus, VerifyStatus};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Number of consecutive failed iterations after which the agent must stop
/// and ask the user for guidance.
const STALL_LIMIT: u32 = 3;

fn today() -> String {
    Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Iteration close-out derived from verify_results. A milestone counts as
/// passed only with an explicit PASS result — absent evidence is a fail.
pub fn run(dir: &Path, reflection: Option<&str>) -> Result<String> {
    let mut st = state::load(dir)?;
    let action = st
        .current_action
        .clone()
        .context("no current_action; nothing to end (run `decide` first)")?;

    let passed = |id: &str| {
        st.verify_results
            .iter()
            .any(|r| r.id == id && r.status == VerifyStatus::Pass)
    };
    let failed_ids: Vec<String> = st
        .verify_results
        .iter()
        .filter(|r| r.status == VerifyStatus::Fail)
        .map(|r| r.id.clone())
        .collect();
    let unverified: Vec<String> = action
        .applicable_milestones
        .iter()
        .filter(|m| !st.verify_results.iter().any(|r| &r.id == *m))
        .cloned()
        .collect();
    let overall_pass = failed_ids.is_empty() && unverified.is_empty();

    let satisfied: Vec<String> = action
        .applicable_milestones
        .iter()
        .filter(|m| passed(m))
        .cloned()
        .collect();

    let mut out = format!("overall: {}", if overall_pass { "PASS" } else { "FAIL" });
    if !failed_ids.is_empty() {
        out.push_str(&format!(" (FAIL: {})", failed_ids.join(", ")));
    }
    if !unverified.is_empty() {
        out.push_str(&format!(" (no verify result for {})", unverified.join(", ")));
    }

    for id in &satisfied {
        let req = st
            .requirements
            .iter_mut()
            .find(|r| r.id == *id)
            .with_context(|| format!("milestone {id} not in registry"))?;
        req.status = Some(ReqStatus::Satisfied);
    }
    out.push_str(&format!("\nsatisfied: {}", if satisfied.is_empty() { "none".into() } else { satisfied.join(", ") }));

    if let Some(sg) = st.subgoals.iter_mut().find(|sg| sg.status == SubgoalStatus::InProgress) {
        let all_passed = sg.milestones.iter().all(|m| satisfied.contains(m));
        if all_passed {
            sg.status = SubgoalStatus::Complete;
            out.push_str(&format!("\nsubgoal {} complete", sg.id));
        } else {
            out.push_str(&format!("\nsubgoal {} still in_progress", sg.id));
        }
    }

    if overall_pass {
        st.stall_count = 0;
    } else {
        st.stall_count += 1;
    }
    out.push_str(&format!("\nstall_count: {}", st.stall_count));
    if st.stall_count >= STALL_LIMIT {
        out.push_str("\nWARNING: 3 consecutive failed iterations — stop and ask the user for guidance");
    }

    // PRD writes: checkbox flips, frontmatter, LOG append — one atomic save.
    let remaining = st
        .requirements
        .iter()
        .filter(|r| r.req_type == ReqType::Milestone && r.status == Some(ReqStatus::Active))
        .count();
    let date = today();
    let closing = st.iteration + 1;
    let prd_path = dir.join(&st.prd_path);
    let mut prd = fs::read_to_string(&prd_path)
        .with_context(|| format!("cannot read PRD {}", prd_path.display()))?;
    prd = prd_md::flip_checkboxes(&prd, &satisfied);

    let mut failing = failed_ids.clone();
    failing.extend(unverified.iter().map(|m| format!("{m} (unverified)")));
    let mut updates = vec![
        (
            "verification_summary",
            format!(
                "\"Iteration {closing}: {}/{} PASS ({})\"",
                satisfied.len(),
                action.applicable_milestones.len(),
                if satisfied.is_empty() { "none".into() } else { satisfied.join(", ") }
            ),
        ),
        (
            "failing_criteria",
            if failing.is_empty() { "none".into() } else { failing.join(", ") },
        ),
        ("last_phase", "UPDATE".into()),
        ("updated", date.clone()),
    ];
    if remaining == 0 {
        updates.push(("status", "COMPLETE".into()));
        out.push_str("\nPRD status: COMPLETE — all milestones satisfied");
    }
    prd = prd_md::update_frontmatter(&prd, &updates)?;

    let invariant_ids: Vec<&str> = st
        .pre_flight_checklist
        .iter()
        .filter(|c| c.req_type == ReqType::Invariant)
        .map(|c| c.id.as_str())
        .collect();
    let invariants_line = if invariant_ids.is_empty() {
        "none registered".to_string()
    } else {
        let inv_fails: Vec<&str> = invariant_ids
            .iter()
            .copied()
            .filter(|id| failed_ids.iter().any(|f| f == id))
            .collect();
        if inv_fails.is_empty() { "all PASS".to_string() } else { format!("FAIL: {}", inv_fails.join(", ")) }
    };
    let mut entry = format!(
        "### Iteration {closing} — {date}\n\
         - **Start commit:** {}\n\
         - **Artifacts:** {} (tier: {})\n\
         - **Milestones addressed:** {}\n\
         - **Invariants verified:** {}\n\
         - **Overall:** {}",
        st.start_commit.as_deref().map(|c| format!("`{c}`")).unwrap_or_else(|| "none".into()),
        action.artifacts.iter().map(|a| format!("`{a}`")).collect::<Vec<_>>().join(", "),
        action.tier,
        action.applicable_milestones.join(", "),
        invariants_line,
        if overall_pass { "PASS" } else { "FAIL" },
    );
    if let Some(r) = reflection {
        entry.push_str(&format!("\n- **Reflection:** {r}"));
    }
    entry.push_str(&format!("\n- **Remaining:** {remaining} milestones pending"));
    prd = prd_md::append_log(&prd, &entry);
    prd_md::save(&prd_path, &prd)?;
    out.push_str(&format!("\nPRD updated: {}", st.prd_path));

    st.iteration += 1;
    st.current_phase = None;
    st.start_commit = None;
    st.current_action = None;
    st.pre_flight_checklist.clear();
    st.verify_results.clear();
    state::save(dir, &st)?;
    out.push_str(&format!("\niteration: {} (between iterations)", st.iteration));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        ChecklistItem, CurrentAction, Phase, Requirement, State, Subgoal, Tier, VerifyResult,
    };
    use tempfile::TempDir;

    const PRD_FIXTURE: &str = "---\ntitle: t\nstatus: ACTIVE\nupdated: 2026-01-01\nverification_summary: \"old\"\nfailing_criteria: old\nlast_phase: ACT\n---\n\n# PRD\n\n- [ ] ISC-X1: first | Verify: test\n- [ ] ISC-X2: second | Verify: test\n\n## LOG\n\n### Iteration 1 — 2026-01-01\n- **Overall:** PASS\n\n### Iteration 2 — 2026-01-02\n- **Overall:** PASS\n";

    fn milestone(id: &str, status: ReqStatus) -> Requirement {
        Requirement {
            id: id.into(),
            req_type: ReqType::Milestone,
            status: Some(status),
            text: format!("text for {id}"),
        }
    }

    fn result(id: &str, status: VerifyStatus) -> VerifyResult {
        VerifyResult { id: id.into(), status, evidence: "cited".into() }
    }

    /// State mid-iteration: SG-1 in_progress over ISC-X1/ISC-X2, phase VERIFY.
    fn mid_iteration_state() -> State {
        let mut st = State::new("PRD-test.md");
        st.iteration = 4;
        st.current_phase = Some(Phase::Verify);
        st.start_commit = Some("abc1234".into());
        st.requirements = vec![
            milestone("ISC-X1", ReqStatus::Active),
            milestone("ISC-X2", ReqStatus::Active),
        ];
        st.subgoals.push(Subgoal {
            id: "SG-1".into(),
            artifacts: vec!["src/x.rs".into()],
            tier: Tier::Standard,
            description: "module x".into(),
            milestones: vec!["ISC-X1".into(), "ISC-X2".into()],
            status: SubgoalStatus::InProgress,
        });
        st.current_action = Some(CurrentAction {
            artifacts: vec!["src/x.rs".into()],
            tier: Tier::Standard,
            description: "module x".into(),
            applicable_milestones: vec!["ISC-X1".into(), "ISC-X2".into()],
        });
        st.pre_flight_checklist = vec![ChecklistItem {
            id: "ISC-X1".into(),
            req_type: ReqType::Milestone,
        }];
        st
    }

    fn save(st: &State) -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("PRD-test.md"), PRD_FIXTURE).unwrap();
        state::save(dir.path(), st).unwrap();
        dir
    }

    fn req_status(st: &State, id: &str) -> ReqStatus {
        st.requirements.iter().find(|r| r.id == id).unwrap().status.unwrap()
    }

    fn prd(dir: &TempDir) -> String {
        fs::read_to_string(dir.path().join("PRD-test.md")).unwrap()
    }

    #[test]
    fn all_pass_satisfies_completes_resets_and_clears() {
        let mut st = mid_iteration_state();
        st.stall_count = 1;
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let dir = save(&st);
        let out = run(dir.path(), None).unwrap();
        assert!(out.contains("overall: PASS"));
        assert!(out.contains("subgoal SG-1 complete"));
        let after = state::load(dir.path()).unwrap();
        assert_eq!(req_status(&after, "ISC-X1"), ReqStatus::Satisfied);
        assert_eq!(req_status(&after, "ISC-X2"), ReqStatus::Satisfied);
        assert_eq!(after.subgoals[0].status, SubgoalStatus::Complete);
        assert_eq!(after.stall_count, 0);
        // ISC-E7: increment + clear transients + null phase
        assert_eq!(after.iteration, 5);
        assert_eq!(after.current_phase, None);
        assert_eq!(after.start_commit, None);
        assert!(after.current_action.is_none());
        assert!(after.pre_flight_checklist.is_empty());
        assert!(after.verify_results.is_empty());
    }

    #[test]
    fn mixed_fail_keeps_subgoal_and_increments_stall() {
        let mut st = mid_iteration_state();
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Fail),
        ];
        let dir = save(&st);
        let out = run(dir.path(), None).unwrap();
        assert!(out.contains("overall: FAIL"));
        assert!(out.contains("subgoal SG-1 still in_progress"));
        let after = state::load(dir.path()).unwrap();
        assert_eq!(req_status(&after, "ISC-X1"), ReqStatus::Satisfied); // passed one still satisfied
        assert_eq!(req_status(&after, "ISC-X2"), ReqStatus::Active);
        assert_eq!(after.subgoals[0].status, SubgoalStatus::InProgress);
        assert_eq!(after.stall_count, 1);
        // ISC-E2: only the passed milestone's checkbox flips
        let prd = prd(&dir);
        assert!(prd.contains("- [x] ISC-X1: first"));
        assert!(prd.contains("- [ ] ISC-X2: second"));
        // ISC-E5: failing_criteria names the failure
        assert!(prd.contains("failing_criteria: ISC-X2"));
        assert!(!prd.contains("status: COMPLETE"));
    }

    #[test]
    fn unverified_milestone_is_a_fail_not_satisfied() {
        let mut st = mid_iteration_state();
        st.verify_results = vec![result("ISC-X1", VerifyStatus::Pass)]; // ISC-X2 never verified
        let dir = save(&st);
        let out = run(dir.path(), None).unwrap();
        assert!(out.contains("overall: FAIL"));
        assert!(out.contains("no verify result for ISC-X2"));
        let after = state::load(dir.path()).unwrap();
        assert_eq!(req_status(&after, "ISC-X2"), ReqStatus::Active);
        assert_eq!(after.subgoals[0].status, SubgoalStatus::InProgress);
        assert_eq!(after.stall_count, 1);
        assert!(prd(&dir).contains("failing_criteria: ISC-X2 (unverified)"));
    }

    #[test]
    fn third_stall_warns_to_stop() {
        let mut st = mid_iteration_state();
        st.stall_count = 2;
        st.verify_results = vec![result("ISC-X1", VerifyStatus::Fail)];
        let dir = save(&st);
        let out = run(dir.path(), None).unwrap();
        assert!(out.contains("stall_count: 3"));
        assert!(out.contains("stop and ask the user"));
    }

    #[test]
    fn no_current_action_errors() {
        let dir = save(&State::new("PRD-test.md"));
        let err = run(dir.path(), None).unwrap_err();
        assert!(err.to_string().contains("no current_action"));
    }

    #[test]
    fn log_entry_appends_last_with_reflection() {
        let mut st = mid_iteration_state();
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let dir = save(&st);
        run(dir.path(), Some("learned a thing")).unwrap();
        let prd = prd(&dir);
        let it2 = prd.find("### Iteration 2").unwrap();
        let it5 = prd.find("### Iteration 5").unwrap(); // closing iteration = 4 + 1
        assert!(it5 > it2, "new entry must follow the last existing one");
        assert!(prd.contains("- **Start commit:** `abc1234`"));
        assert!(prd.contains("- **Artifacts:** `src/x.rs` (tier: standard)"));
        assert!(prd.contains("- **Milestones addressed:** ISC-X1, ISC-X2"));
        assert!(prd.contains("- **Reflection:** learned a thing"));
        assert!(prd.contains("- **Remaining:** 0 milestones pending"));
    }

    #[test]
    fn frontmatter_updated_and_complete_when_all_satisfied() {
        let mut st = mid_iteration_state();
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let dir = save(&st);
        let out = run(dir.path(), None).unwrap();
        let prd = prd(&dir);
        // ISC-E5
        assert!(prd.contains("verification_summary: \"Iteration 5: 2/2 PASS (ISC-X1, ISC-X2)\""));
        assert!(prd.contains("failing_criteria: none"));
        assert!(prd.contains("last_phase: UPDATE"));
        assert!(!prd.contains("updated: 2026-01-01"));
        // ISC-E8: both registry milestones satisfied → COMPLETE
        assert!(prd.contains("status: COMPLETE"));
        assert!(out.contains("PRD status: COMPLETE"));
    }

    #[test]
    fn not_complete_while_other_milestones_active() {
        let mut st = mid_iteration_state();
        st.requirements.push(milestone("ISC-Y1", ReqStatus::Active)); // outside this subgoal
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let dir = save(&st);
        run(dir.path(), None).unwrap();
        let prd = prd(&dir);
        assert!(prd.contains("status: ACTIVE"));
        assert!(prd.contains("- **Remaining:** 1 milestones pending"));
    }
}
