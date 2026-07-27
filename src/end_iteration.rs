//! UPDATE: close out an iteration — registry bookkeeping, subgoal status,
//! stall handling, PRD markdown writes, and transient-field reset.

use crate::prd_md;
use crate::state::{State, SubgoalStatus, VerifyStatus};
#[cfg(test)]
use crate::state; // test module reaches state::load/save through this alias
use anyhow::{bail, Context, Result};
use std::fmt::Write as _;
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
        .filter(|o| o.status.success()).map_or_else(|| "unknown".into(), |o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Iteration close-out computed from state — the pure policy core. A milestone
/// counts as passed only with an explicit PASS result; absent evidence is a fail.
#[derive(Debug, PartialEq)]
pub struct IterationOutcome {
    pub overall_pass: bool,
    pub satisfied: Vec<String>,
    pub failed: Vec<String>,
    pub unverified: Vec<String>,
    pub next_stall_count: u32,
    /// The in-progress subgoal, if any, and whether this outcome completes it.
    pub in_progress_subgoal: Option<String>,
    pub subgoal_complete: bool,
}

pub fn outcome(st: &State) -> Result<IterationOutcome> {
    let action = st
        .current_action
        .as_ref()
        .context("no current_action; nothing to end (run `decide` first)")?;
    let passed = |id: &str| {
        st.verify_results
            .iter()
            .any(|r| r.id == id && r.status == VerifyStatus::Pass)
    };
    let failed: Vec<String> = st
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
    let overall_pass = failed.is_empty() && unverified.is_empty();
    let satisfied: Vec<String> = action
        .applicable_milestones
        .iter()
        .filter(|m| passed(m))
        .cloned()
        .collect();
    let in_progress = st.subgoals.iter().find(|sg| sg.status == SubgoalStatus::InProgress);
    Ok(IterationOutcome {
        overall_pass,
        next_stall_count: if overall_pass { 0 } else { st.stall_count + 1 },
        subgoal_complete: in_progress
            .is_some_and(|sg| sg.milestones.iter().all(|m| satisfied.contains(m))),
        in_progress_subgoal: in_progress.map(|sg| sg.id.clone()),
        satisfied,
        failed,
        unverified,
    })
}

/// Apply the outcome: registry and subgoal bookkeeping, PRD writes,
/// transient-field reset. Thin I/O shell around `outcome`.
pub fn run(dir: &Path, reflection: Option<&str>, gate: Option<&str>) -> Result<String> {
    State::update(dir, |st| {
        let o = outcome(st)?;
        let action = st
            .current_action
            .clone()
            .context("no current_action; nothing to end (run `decide` first)")?;

        // A PASS iteration must record its load-bearing insight; bail before any
        // state or PRD write so a missing reflection never half-closes the loop.
        // Bailing here returns the closure Err, so `update` skips the save too.
        if o.overall_pass && reflection.is_none() {
            bail!("--reflection is required on a PASS iteration (use --reflection \"\" for no note)");
        }

        let mut out = format!("overall: {}", if o.overall_pass { "PASS" } else { "FAIL" });
        if !o.failed.is_empty() {
            let _ = write!(out, " (FAIL: {})", o.failed.join(", "));
        }
        if !o.unverified.is_empty() {
            let _ = write!(out, " (no verify result for {})", o.unverified.join(", "));
        }

        for id in &o.satisfied {
            st.requirements.mark_satisfied(id)?;
        }
        let _ = write!(out, "\nsatisfied: {}", if o.satisfied.is_empty() { "none".into() } else { o.satisfied.join(", ") });

        if let Some(sg_id) = &o.in_progress_subgoal {
            if o.subgoal_complete {
                let sg = st
                    .subgoals
                    .iter_mut()
                    .find(|sg| sg.id == *sg_id)
                    .context("in-progress subgoal vanished from state during end-iteration")?;
                sg.status = SubgoalStatus::Complete;
                let _ = write!(out, "\nsubgoal {sg_id} complete");
            } else {
                let _ = write!(out, "\nsubgoal {sg_id} still in_progress");
            }
        }

        st.stall_count = o.next_stall_count;
        let _ = write!(out, "\nstall_count: {}", st.stall_count);
        if st.stall_count >= STALL_LIMIT {
            out.push_str("\nWARNING: 3 consecutive failed iterations — stop and ask the user for guidance");
        }

        // PRD writes: checkbox flips, frontmatter, LOG append. This side effect
        // stays inside the closure, before the field reset; `update` saves
        // state.json after, preserving today's PRD-first, non-atomic ordering.
        let remaining = st.requirements.pending_milestones();
        let date = today();
        let closing = st.iteration + 1;
        let prd_path = dir.join(&st.prd_path);
        let mut prd = fs::read_to_string(&prd_path)
            .with_context(|| format!("cannot read PRD {}", prd_path.display()))?;
        prd = prd_md::flip_checkboxes(&prd, &o.satisfied);

        let mut failing = o.failed.clone();
        failing.extend(o.unverified.iter().map(|m| format!("{m} (unverified)")));
        let mut updates = vec![
            (
                "verification_summary",
                format!(
                    "\"Iteration {closing}: {}/{} PASS ({})\"",
                    o.satisfied.len(),
                    action.applicable_milestones.len(),
                    if o.satisfied.is_empty() { "none".into() } else { o.satisfied.join(", ") }
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

        let entry = log_entry(st, &o, reflection, gate, closing, &date);
        prd = prd_md::append_log(&prd, &entry);
        prd_md::save(&prd_path, &prd)?;
        let _ = write!(out, "\nPRD updated: {}", st.prd_path);

        st.begin_next_iteration();
        let _ = write!(out, "\niteration: {} (between iterations)", st.iteration);
        Ok(out)
    })
}

/// One dense bullet per iteration (grammar in the PRD).
fn log_entry(
    st: &State,
    o: &IterationOutcome,
    reflection: Option<&str>,
    gate: Option<&str>,
    closing: u32,
    date: &str,
) -> String {
    // `start_commit` is the OBSERVE HEAD (parent of this iteration's own commit,
    // which end-iteration runs before); render it on PASS only.
    let commit = if o.overall_pass {
        st.start_commit.as_deref().map_or_else(String::new, |c| format!("`{c}` · "))
    } else {
        String::new()
    };
    let sg = o.in_progress_subgoal.as_deref().unwrap_or("(no subgoal)");
    let refl = match reflection {
        Some(r) if !r.is_empty() => format!(" — {r}"),
        _ => String::new(),
    };
    let satisfied = if o.satisfied.is_empty() { "none".to_string() } else { o.satisfied.join(", ") };
    let gate_seg = match gate {
        Some(g) if !g.is_empty() => format!("; {g}"),
        _ => String::new(),
    };
    let fail_seg = if o.overall_pass {
        String::new()
    } else {
        let reason = o.failed.iter().chain(&o.unverified).cloned().collect::<Vec<_>>().join(", ");
        format!("; FAIL: {reason}")
    };
    format!("- **{closing}** · {date} · {commit}{sg}{refl} → {satisfied} satisfied{gate_seg}{fail_seg}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        ChecklistItem, CurrentAction, Phase, ReqStatus, ReqType, Subgoal, Tier, VerifyResult,
    };
    use tempfile::TempDir;

    const PRD_FIXTURE: &str = "---\ntitle: t\nstatus: ACTIVE\nupdated: 2026-01-01\nverification_summary: \"old\"\nfailing_criteria: old\nlast_phase: ACT\n---\n\n# PRD\n\n- [ ] ISC-X1: first | Verify: test\n- [ ] ISC-X2: second | Verify: test\n\n## LOG\n\n### Iteration 1 — 2026-01-01\n- **Overall:** PASS\n\n### Iteration 2 — 2026-01-02\n- **Overall:** PASS\n";

    fn result(id: &str, status: VerifyStatus) -> VerifyResult {
        VerifyResult { id: id.into(), status, evidence: "cited".into() }
    }

    /// State mid-iteration: SG-1 `in_progress` over ISC-X1/ISC-X2, phase VERIFY.
    fn mid_iteration_state() -> State {
        let mut st = State::new("PRD-test.md");
        st.iteration = 4;
        st.current_phase = Some(Phase::Verify);
        st.start_commit = Some("abc1234".into());
        st.requirements.add("ISC-X1", ReqType::Milestone, "text for ISC-X1", None).unwrap();
        st.requirements.add("ISC-X2", ReqType::Milestone, "text for ISC-X2", None).unwrap();
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
        st.requirements.find(id).unwrap().status.unwrap()
    }

    fn prd(dir: &TempDir) -> String {
        fs::read_to_string(dir.path().join("PRD-test.md")).unwrap()
    }

    // Pure-core tests: verdict policy without TempDir or PRD fixtures.

    #[test]
    fn outcome_all_pass_resets_stall_and_completes_subgoal() {
        let mut st = mid_iteration_state();
        st.stall_count = 2;
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let o = outcome(&st).unwrap();
        assert!(o.overall_pass);
        assert_eq!(o.satisfied, vec!["ISC-X1", "ISC-X2"]);
        assert_eq!(o.next_stall_count, 0);
        assert_eq!(o.in_progress_subgoal.as_deref(), Some("SG-1"));
        assert!(o.subgoal_complete);
    }

    #[test]
    fn outcome_unverified_milestone_is_a_fail() {
        let mut st = mid_iteration_state();
        st.verify_results = vec![result("ISC-X1", VerifyStatus::Pass)]; // ISC-X2 never verified
        let o = outcome(&st).unwrap();
        assert!(!o.overall_pass);
        assert_eq!(o.unverified, vec!["ISC-X2"]);
        assert_eq!(o.satisfied, vec!["ISC-X1"]);
        assert_eq!(o.next_stall_count, 1);
        assert!(!o.subgoal_complete);
    }

    #[test]
    fn outcome_explicit_fail_increments_stall() {
        let mut st = mid_iteration_state();
        st.stall_count = 1;
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Fail),
        ];
        let o = outcome(&st).unwrap();
        assert!(!o.overall_pass);
        assert_eq!(o.failed, vec!["ISC-X2"]);
        assert_eq!(o.next_stall_count, 2);
    }

    #[test]
    fn outcome_without_current_action_errors() {
        let err = outcome(&State::new("PRD-test.md")).unwrap_err();
        assert!(err.to_string().contains("no current_action"));
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
        let out = run(dir.path(), Some("done"), None).unwrap();
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
        let out = run(dir.path(), None, None).unwrap();
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
        let out = run(dir.path(), None, None).unwrap();
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
        let out = run(dir.path(), None, None).unwrap();
        assert!(out.contains("stall_count: 3"));
        assert!(out.contains("stop and ask the user"));
    }

    #[test]
    fn no_current_action_errors() {
        let dir = save(&State::new("PRD-test.md"));
        let err = run(dir.path(), None, None).unwrap_err();
        assert!(err.to_string().contains("no current_action"));
    }

    // ISC-LOG-1/2: end-to-end, the appended entry is one bullet after the last
    // existing entry — no `### Iteration` heading, no Artifacts/Tier/etc bullets.
    #[test]
    fn end_iteration_appends_one_bullet_after_last_entry() {
        let mut st = mid_iteration_state();
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let dir = save(&st);
        run(dir.path(), Some("learned a thing"), Some("cargo test ok")).unwrap();
        let prd = prd(&dir);
        let it2 = prd.find("### Iteration 2").unwrap();
        let bullet = prd.find("- **5**").unwrap(); // closing iteration = 4 + 1
        assert!(bullet > it2, "new entry must follow the last existing one");
        assert!(prd.contains(
            "- **5** · "
        ));
        assert!(prd.contains(
            "`abc1234` · SG-1 — learned a thing → ISC-X1, ISC-X2 satisfied; cargo test ok"
        ));
        assert!(!prd.contains("### Iteration 5"), "no per-iteration heading");
        assert!(!prd.contains("- **Artifacts:**"));
        assert!(!prd.contains("- **Remaining:**"));
    }

    #[test]
    fn frontmatter_updated_and_complete_when_all_satisfied() {
        let mut st = mid_iteration_state();
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let dir = save(&st);
        let out = run(dir.path(), Some("done"), None).unwrap();
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
        // outside this subgoal
        st.requirements.add("ISC-Y1", ReqType::Milestone, "text for ISC-Y1", None).unwrap();
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let dir = save(&st);
        run(dir.path(), Some("done"), None).unwrap();
        let prd = prd(&dir);
        assert!(prd.contains("status: ACTIVE"));
    }

    // ---- one-bullet grammar (pure log_entry) ----

    fn state_with_commit(commit: Option<&str>) -> State {
        let mut st = State::new("PRD.md");
        st.start_commit = commit.map(str::to_string);
        st
    }

    fn pass_outcome(sg: &str, satisfied: &[&str]) -> IterationOutcome {
        IterationOutcome {
            overall_pass: true,
            satisfied: satisfied.iter().map(|s| (*s).to_string()).collect(),
            failed: vec![],
            unverified: vec![],
            next_stall_count: 0,
            in_progress_subgoal: Some(sg.to_string()),
            subgoal_complete: true,
        }
    }

    // ISC-LOG-1/2/3/4: full bullet, PASS with commit + reflection + gate.
    #[test]
    fn log_entry_pass_full_bullet() {
        let st = state_with_commit(Some("a1b2c3d"));
        let o = pass_outcome("SG-2", &["ISC-2", "ISC-3"]);
        let s = log_entry(&st, &o, Some("sum path is the seam"), Some("RED 1→GREEN 7, cargo test ok"), 7, "2026-07-03");
        assert_eq!(
            s,
            "- **7** · 2026-07-03 · `a1b2c3d` · SG-2 — sum path is the seam → ISC-2, ISC-3 satisfied; RED 1→GREEN 7, cargo test ok"
        );
        assert!(!s.contains("### Iteration"));
        assert!(!s.contains('\n'), "one-bullet entry is a single line");
    }

    // ISC-LOG-2: start_commit is none -> no backticked hash on a PASS bullet.
    #[test]
    fn log_entry_pass_without_commit_omits_hash() {
        let st = state_with_commit(None);
        let o = pass_outcome("SG-1", &["ISC-1"]);
        let s = log_entry(&st, &o, Some("note"), None, 3, "2026-07-03");
        assert_eq!(s, "- **3** · 2026-07-03 · SG-1 — note → ISC-1 satisfied");
    }

    // ISC-LOG-5 (render half): empty reflection collapses the `— … ` segment.
    #[test]
    fn log_entry_empty_reflection_collapses_segment() {
        let st = state_with_commit(Some("abc1234"));
        let o = pass_outcome("SG-1", &["ISC-1"]);
        let s = log_entry(&st, &o, Some(""), None, 4, "2026-07-03");
        assert_eq!(s, "- **4** · 2026-07-03 · `abc1234` · SG-1 → ISC-1 satisfied");
    }

    // ISC-LOG-2/3: FAIL omits the commit hash and appends failed + unverified ids.
    #[test]
    fn log_entry_fail_omits_hash_and_lists_failures() {
        let st = state_with_commit(Some("abc1234")); // present, but FAIL -> omitted
        let o = IterationOutcome {
            overall_pass: false,
            satisfied: vec!["ISC-1".into()],
            failed: vec!["ISC-2".into()],
            unverified: vec!["ISC-3".into()],
            next_stall_count: 1,
            in_progress_subgoal: Some("SG-3".into()),
            subgoal_complete: false,
        };
        let s = log_entry(&st, &o, None, Some("RED 2→GREEN 1"), 5, "2026-07-03");
        assert_eq!(
            s,
            "- **5** · 2026-07-03 · SG-3 → ISC-1 satisfied; RED 2→GREEN 1; FAIL: ISC-2, ISC-3"
        );
    }

    // ISC-LOG-4: no gate flag -> no `; ` gate segment.
    #[test]
    fn log_entry_no_gate_has_no_gate_segment() {
        let st = state_with_commit(None);
        let o = pass_outcome("SG-1", &["ISC-1"]);
        let s = log_entry(&st, &o, Some("n"), None, 2, "2026-07-03");
        assert_eq!(s, "- **2** · 2026-07-03 · SG-1 — n → ISC-1 satisfied");
    }

    // ---- reflection-required contract (ISC-LOG-5) ----

    #[test]
    fn pass_without_reflection_errors_and_writes_nothing() {
        let mut st = mid_iteration_state();
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let dir = save(&st);
        let before = prd(&dir);
        let err = run(dir.path(), None, None).unwrap_err();
        assert!(err.to_string().contains("--reflection is required"));
        assert_eq!(prd(&dir), before, "PRD must be untouched");
        let after = state::load(dir.path()).unwrap();
        assert_eq!(after.iteration, 4, "iteration must not advance");
        assert_eq!(after.subgoals[0].status, SubgoalStatus::InProgress);
        assert_eq!(req_status(&after, "ISC-X1"), ReqStatus::Active);
    }

    #[test]
    fn pass_with_empty_reflection_is_accepted() {
        let mut st = mid_iteration_state();
        st.verify_results = vec![
            result("ISC-X1", VerifyStatus::Pass),
            result("ISC-X2", VerifyStatus::Pass),
        ];
        let dir = save(&st);
        run(dir.path(), Some(""), None).unwrap();
        let prd = prd(&dir);
        assert!(prd.contains("→ ISC-X1, ISC-X2 satisfied"));
        assert!(!prd.contains("SG-1 — "), "empty reflection collapses the dash segment");
    }
}
