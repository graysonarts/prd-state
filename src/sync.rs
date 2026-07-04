//! Sync the requirements registry from the invariant doc and the PRD.
//! Doc is authoritative for invariants; PRD checkboxes are authoritative for milestones.
//! Grammar lives in `prd_md`; the diff lives on `state::Registry`. This module is I/O glue.

use crate::prd_md;
use crate::state::{self, ReqType, State};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Load state, parse the invariant doc (if present) and the PRD, diff, save, report.
pub fn run(dir: &Path, invariant_doc: Option<&Path>) -> Result<String> {
    // prd_path lives in state; this read-only load locates the PRD before parsing.
    let prd_path = dir.join(state::load(dir)?.prd_path);
    let mut parsed = Vec::new();
    let mut doc_note = String::new();
    match invariant_doc {
        Some(doc) if doc.is_file() => {
            let md = fs::read_to_string(doc)
                .with_context(|| format!("cannot read {}", doc.display()))?;
            parsed.extend(
                prd_md::parse_requirements(&md)
                    .with_context(|| format!("in invariant doc {}", doc.display()))?
                    .into_iter()
                    .filter(|p| p.req_type == ReqType::Invariant),
            );
        }
        _ => doc_note = " (invariant doc not found; 0 invariants loaded)".to_string(),
    }
    let prd = fs::read_to_string(&prd_path)
        .with_context(|| format!("cannot read PRD {}", prd_path.display()))?;
    for p in prd_md::parse_requirements(&prd)
        .with_context(|| format!("in PRD {}", prd_path.display()))?
    {
        // Dedup: doc wins for invariants already loaded.
        if !parsed.iter().any(|q| q.id == p.id) {
            parsed.push(p);
        }
    }
    State::update(dir, |st| {
        let report = st.requirements.upsert_from_parsed(&parsed);
        Ok(format!("{report}{doc_note}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use tempfile::TempDir;

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
