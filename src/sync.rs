//! Sync the requirements registry from the invariant doc and the PRD.
//! Doc is authoritative for invariants; PRD checkboxes are authoritative for milestones.

use crate::state::{self, ReqStatus, ReqType, Requirement};
use anyhow::{bail, Context, Result};
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct ParsedReq {
    pub id: String,
    pub req_type: ReqType,
    pub text: String,
}

/// Extract requirement lines (`- [ ] INV-X: text | Verify: method`) from markdown.
/// Non-requirement lines are ignored; a malformed requirement line is a hard error.
pub fn parse_requirements(md: &str) -> Result<Vec<ParsedReq>> {
    let mut out = Vec::new();
    for (n, line) in md.lines().enumerate() {
        let rest = match line
            .strip_prefix("- [ ] ")
            .or_else(|| line.strip_prefix("- [x] "))
        {
            Some(r) => r,
            None => continue,
        };
        let req_type = if rest.starts_with("INV-") {
            ReqType::Invariant
        } else if rest.starts_with("ISC-") {
            ReqType::Milestone
        } else {
            continue; // checkbox line but not a requirement
        };
        let parsed = rest.split_once(": ").and_then(|(id, tail)| {
            let text = tail.split_once(" | Verify:").map(|(t, _)| t).unwrap_or(tail);
            let text = text.trim();
            (!id.contains(' ') && !text.is_empty()).then(|| ParsedReq {
                id: id.trim_end_matches(':').to_string(),
                req_type,
                text: text.to_string(),
            })
        });
        match parsed {
            Some(p) => out.push(p),
            None => bail!(
                "line {}: cannot parse requirement line {line:?}; register it manually with `prd-state req add`",
                n + 1
            ),
        }
    }
    Ok(out)
}

#[derive(Debug, Default, PartialEq)]
pub struct SyncReport {
    pub added_invariants: usize,
    pub added_milestones: usize,
    pub removed: usize,
    pub unchanged: usize,
}

impl fmt::Display for SyncReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "sync: +{} invariants, +{} milestones, {} removed, {} unchanged",
            self.added_invariants, self.added_milestones, self.removed, self.unchanged
        )
    }
}

/// Diff parsed requirements into the registry. Pure; no I/O.
pub fn apply(reqs: &mut Vec<Requirement>, parsed: &[ParsedReq]) -> SyncReport {
    let mut report = SyncReport::default();
    for p in parsed {
        match reqs.iter_mut().find(|r| r.id == p.id) {
            None => {
                let status = match p.req_type {
                    ReqType::Invariant => None,
                    ReqType::Milestone => Some(ReqStatus::Active),
                };
                reqs.push(Requirement {
                    id: p.id.clone(),
                    req_type: p.req_type,
                    status,
                    text: p.text.clone(),
                });
                match p.req_type {
                    ReqType::Invariant => report.added_invariants += 1,
                    ReqType::Milestone => report.added_milestones += 1,
                }
            }
            Some(existing) => {
                existing.text = p.text.clone(); // source is authoritative for wording
                if existing.status == Some(ReqStatus::Removed) {
                    existing.status = Some(ReqStatus::Active); // reappeared in PRD
                    report.added_milestones += 1;
                } else {
                    report.unchanged += 1;
                }
            }
        }
    }
    // Milestones absent from the PRD are removed. Invariants are never removed by sync.
    for r in reqs.iter_mut() {
        if r.req_type == ReqType::Milestone
            && r.status != Some(ReqStatus::Removed)
            && !parsed.iter().any(|p| p.id == r.id)
        {
            r.status = Some(ReqStatus::Removed);
            report.removed += 1;
        }
    }
    report
}

/// Load state, parse the invariant doc (if present) and the PRD, diff, save, report.
pub fn run(dir: &Path, invariant_doc: Option<&Path>) -> Result<String> {
    let mut st = state::load(dir)?;
    let mut parsed = Vec::new();
    let mut doc_note = String::new();
    match invariant_doc {
        Some(doc) if doc.is_file() => {
            let md = fs::read_to_string(doc)
                .with_context(|| format!("cannot read {}", doc.display()))?;
            parsed.extend(
                parse_requirements(&md)
                    .with_context(|| format!("in invariant doc {}", doc.display()))?
                    .into_iter()
                    .filter(|p| p.req_type == ReqType::Invariant),
            );
        }
        _ => doc_note = " (invariant doc not found; 0 invariants loaded)".to_string(),
    }
    let prd_path = dir.join(&st.prd_path);
    let prd = fs::read_to_string(&prd_path)
        .with_context(|| format!("cannot read PRD {}", prd_path.display()))?;
    for p in parse_requirements(&prd).with_context(|| format!("in PRD {}", prd_path.display()))? {
        // Dedup: doc wins for invariants already loaded.
        if !parsed.iter().any(|q| q.id == p.id) {
            parsed.push(p);
        }
    }
    let report = apply(&mut st.requirements, &parsed);
    state::save(dir, &st)?;
    Ok(format!("{report}{doc_note}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use tempfile::TempDir;

    #[test]
    fn parses_checked_unchecked_and_ignores_prose() {
        let md = "# Title\nprose here\n- [ ] ISC-A1: does a thing | Verify: Test: unit\n- [x] INV-B2: never breaks | Verify: Grep\n- [ ] plain checkbox, not a requirement\n";
        let reqs = parse_requirements(md).unwrap();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0], ParsedReq { id: "ISC-A1".into(), req_type: ReqType::Milestone, text: "does a thing".into() });
        assert_eq!(reqs[1], ParsedReq { id: "INV-B2".into(), req_type: ReqType::Invariant, text: "never breaks".into() });
    }

    #[test]
    fn malformed_line_errors_with_line_number_and_fallback() {
        let md = "ok\n- [ ] ISC-BAD no colon separator\n";
        let err = parse_requirements(md).unwrap_err().to_string();
        assert!(err.contains("line 2"), "{err}");
        assert!(err.contains("ISC-BAD"), "{err}");
        assert!(err.contains("req add"), "{err}");
    }

    #[test]
    fn apply_registers_invariants_deduplicating() {
        let mut reqs = vec![Requirement {
            id: "INV-A1".into(),
            req_type: ReqType::Invariant,
            status: None,
            text: "old wording".into(),
        }];
        let parsed = vec![
            ParsedReq { id: "INV-A1".into(), req_type: ReqType::Invariant, text: "new wording".into() },
            ParsedReq { id: "INV-A2".into(), req_type: ReqType::Invariant, text: "b".into() },
            ParsedReq { id: "INV-A3".into(), req_type: ReqType::Invariant, text: "c".into() },
        ];
        let report = apply(&mut reqs, &parsed);
        assert_eq!(report.added_invariants, 2);
        assert_eq!(report.unchanged, 1);
        assert_eq!(reqs.len(), 3);
        assert_eq!(reqs[0].text, "new wording"); // doc authoritative
    }

    #[test]
    fn apply_diffs_milestones_preserving_satisfied() {
        let mut reqs = vec![
            Requirement { id: "ISC-A".into(), req_type: ReqType::Milestone, status: Some(ReqStatus::Satisfied), text: "a".into() },
            Requirement { id: "ISC-B".into(), req_type: ReqType::Milestone, status: Some(ReqStatus::Active), text: "b".into() },
        ];
        // PRD now has A and C; B is gone.
        let parsed = vec![
            ParsedReq { id: "ISC-A".into(), req_type: ReqType::Milestone, text: "a".into() },
            ParsedReq { id: "ISC-C".into(), req_type: ReqType::Milestone, text: "c".into() },
        ];
        let report = apply(&mut reqs, &parsed);
        assert_eq!(report.added_milestones, 1);
        assert_eq!(report.removed, 1);
        assert_eq!(report.unchanged, 1);
        let get = |id: &str| reqs.iter().find(|r| r.id == id).unwrap().status;
        assert_eq!(get("ISC-A"), Some(ReqStatus::Satisfied));
        assert_eq!(get("ISC-B"), Some(ReqStatus::Removed));
        assert_eq!(get("ISC-C"), Some(ReqStatus::Active));
    }

    #[test]
    fn removed_milestone_reactivates_when_back_in_prd() {
        let mut reqs = vec![Requirement {
            id: "ISC-A".into(),
            req_type: ReqType::Milestone,
            status: Some(ReqStatus::Removed),
            text: "a".into(),
        }];
        let parsed = vec![ParsedReq { id: "ISC-A".into(), req_type: ReqType::Milestone, text: "a".into() }];
        apply(&mut reqs, &parsed);
        assert_eq!(reqs[0].status, Some(ReqStatus::Active));
    }

    #[test]
    fn run_reports_counts_and_missing_doc() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("PRD-test.md"),
            "# PRD\n- [ ] ISC-A: first | Verify: Test\n- [ ] ISC-B: second | Verify: Test\n",
        )
        .unwrap();
        state::save(dir.path(), &State::new("PRD-test.md")).unwrap();
        let msg = run(dir.path(), Some(&dir.path().join("no-doc.md"))).unwrap();
        assert!(msg.contains("+0 invariants, +2 milestones, 0 removed, 0 unchanged"), "{msg}");
        assert!(msg.contains("invariant doc not found"), "{msg}");
        assert_eq!(state::load(dir.path()).unwrap().requirements.len(), 2);
    }

    #[test]
    fn run_loads_invariants_from_doc() {
        let dir = TempDir::new().unwrap();
        let doc = dir.path().join("invariants.md");
        std::fs::write(&doc, "- [ ] INV-T1: tests pass | Verify: Test\n").unwrap();
        std::fs::write(dir.path().join("PRD-test.md"), "- [ ] INV-T1: tests pass | Verify: Test\n- [ ] ISC-A: a | Verify: Test\n").unwrap();
        state::save(dir.path(), &State::new("PRD-test.md")).unwrap();
        let msg = run(dir.path(), Some(&doc)).unwrap();
        // INV-T1 appears in both doc and PRD -> registered once
        assert!(msg.contains("+1 invariants, +1 milestones"), "{msg}");
        let st = state::load(dir.path()).unwrap();
        assert_eq!(st.requirements.iter().filter(|r| r.id == "INV-T1").count(), 1);
    }
}
