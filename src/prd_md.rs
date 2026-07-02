//! Pure PRD-markdown transforms: checkbox flips, LOG append, frontmatter
//! updates. All functions are &str -> String; file I/O stays in callers.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Flip `- [ ] <id>:` to `- [x] <id>:` for the given ids only.
pub fn flip_checkboxes(prd: &str, ids: &[String]) -> String {
    let mut out = prd.to_string();
    for id in ids {
        out = out.replace(&format!("- [ ] {id}:"), &format!("- [x] {id}:"));
    }
    out
}

/// Append a LOG entry at the true end of the file — LOG entries are always
/// the file's tail, so end-of-file append can never reverse their order.
pub fn append_log(prd: &str, entry: &str) -> String {
    format!("{}\n\n{}\n", prd.trim_end(), entry.trim_end())
}

/// Replace `key: value` lines inside the leading `---` frontmatter block.
/// Keys absent from the block are an error — the PRD template defines them.
pub fn update_frontmatter(prd: &str, updates: &[(&str, String)]) -> Result<String> {
    let rest = prd.strip_prefix("---\n").context("PRD has no frontmatter")?;
    let (fm, body) = rest.split_once("\n---\n").context("unterminated frontmatter")?;
    let mut lines: Vec<String> = fm.lines().map(str::to_string).collect();
    for (key, value) in updates {
        let line = lines
            .iter_mut()
            .find(|l| l.starts_with(&format!("{key}:")))
            .with_context(|| format!("frontmatter key not found: {key}"))?;
        *line = format!("{key}: {value}");
    }
    Ok(format!("---\n{}\n---\n{}", lines.join("\n"), body))
}

/// Atomically write the PRD: temp file in the same dir, then rename.
pub fn save(path: &Path, content: &str) -> Result<()> {
    let dir = path.parent().context("PRD path has no parent directory")?;
    let tmp = dir.join(".prd.md.tmp");
    fs::write(&tmp, content).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("cannot rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRD: &str = "---\ntitle: t\nstatus: ACTIVE\nupdated: 2026-01-01\nverification_summary: \"old\"\nfailing_criteria: none\nlast_phase: ACT\n---\n\n# PRD\n\n- [ ] ISC-A1: first | Verify: test\n- [ ] ISC-A2: second | Verify: test\n\n## LOG\n\n### Iteration 1 — 2026-01-01\n- **Overall:** PASS\n\n### Iteration 2 — 2026-01-02\n- **Overall:** PASS\n";

    #[test]
    fn flip_checkboxes_flips_only_named_ids() {
        let out = flip_checkboxes(PRD, &["ISC-A1".into()]);
        assert!(out.contains("- [x] ISC-A1: first"));
        assert!(out.contains("- [ ] ISC-A2: second"));
    }

    #[test]
    fn append_log_lands_after_last_entry() {
        let out = append_log(PRD, "### Iteration 3 — 2026-01-03\n- **Overall:** FAIL");
        let it2 = out.find("### Iteration 2").unwrap();
        let it3 = out.find("### Iteration 3").unwrap();
        assert!(it3 > it2, "new entry must follow the last existing one");
        assert!(out.ends_with("- **Overall:** FAIL\n"));
    }

    #[test]
    fn update_frontmatter_replaces_values_in_place() {
        let out = update_frontmatter(
            PRD,
            &[
                ("status", "COMPLETE".into()),
                ("updated", "2026-07-02".into()),
                ("verification_summary", "\"Iteration 3: 1/1 PASS\"".into()),
            ],
        )
        .unwrap();
        assert!(out.contains("status: COMPLETE"));
        assert!(out.contains("updated: 2026-07-02"));
        assert!(out.contains("verification_summary: \"Iteration 3: 1/1 PASS\""));
        assert!(out.contains("last_phase: ACT")); // untouched key survives
        assert!(out.contains("# PRD")); // body intact
    }

    #[test]
    fn update_frontmatter_unknown_key_errors() {
        let err = update_frontmatter(PRD, &[("nope", "x".into())]).unwrap_err();
        assert!(err.to_string().contains("frontmatter key not found: nope"));
    }

    #[test]
    fn update_frontmatter_missing_block_errors() {
        assert!(update_frontmatter("# no frontmatter\n", &[]).is_err());
    }
}
