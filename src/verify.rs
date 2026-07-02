//! VERIFY: record per-requirement evidence as it is produced.

use crate::state::{self, VerifyResult, VerifyStatus};
use anyhow::{bail, Result};
use std::path::Path;

/// Append one verify result. Evidence is mandatory — a bare checkmark is a FAIL.
pub fn run(dir: &Path, id: &str, status: VerifyStatus, evidence: &str) -> Result<String> {
    if evidence.trim().is_empty() {
        bail!("evidence required: cite the line, test, or search that proves {id}");
    }
    let mut st = state::load(dir)?;
    st.verify_results.push(VerifyResult {
        id: id.to_string(),
        status,
        evidence: evidence.to_string(),
    });
    let n = st.verify_results.len();
    state::save(dir, &st)?;
    Ok(format!("{id}: {status:?} recorded ({n} result{})", if n == 1 { "" } else { "s" }))
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
    fn verify_appends_results_in_order() {
        let dir = dir_with_state();
        run(dir.path(), "ISC-A1", VerifyStatus::Pass, "test x asserts y").unwrap();
        run(dir.path(), "ISC-A2", VerifyStatus::Fail, "grep found forbidden import").unwrap();
        let results = state::load(dir.path()).unwrap().verify_results;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "ISC-A1");
        assert_eq!(results[0].status, VerifyStatus::Pass);
        assert_eq!(results[0].evidence, "test x asserts y");
        assert_eq!(results[1].id, "ISC-A2");
        assert_eq!(results[1].status, VerifyStatus::Fail);
    }

    #[test]
    fn verify_rejects_empty_evidence() {
        let dir = dir_with_state();
        for bad in ["", "   "] {
            let err = run(dir.path(), "ISC-A1", VerifyStatus::Pass, bad).unwrap_err();
            assert!(err.to_string().contains("evidence required"));
        }
        assert!(state::load(dir.path()).unwrap().verify_results.is_empty());
    }
}
