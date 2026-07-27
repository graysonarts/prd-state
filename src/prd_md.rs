//! The PRD grammar module: requirement-line parsing plus pure markdown
//! transforms (checkbox flips, LOG append, frontmatter updates). Read and
//! write of the requirement-line syntax live here so they agree by
//! construction. Transforms are &str -> String; file I/O stays in callers.

use crate::state::ReqType;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct ParsedReq {
    pub id: String,
    pub req_type: ReqType,
    pub text: String,
    pub annotation: Option<String>,
}

/// Extract requirement lines from markdown, per the grammar
/// `- [ |x] (INV|ISC)-<id>[ (<annotation>)]: <text> | Verify: <method>`.
///
/// The annotation is at most one non-empty parenthesised run immediately before
/// `": "`, containing neither `)` nor `:`. The `:` ban is what lets the split on
/// the first `": "` find the ID region; because that split happens first,
/// parentheses in `<text>` are never examined and stay text.
///
/// Non-requirement lines are ignored; a malformed requirement line is a hard error.
pub fn parse_requirements(md: &str) -> Result<Vec<ParsedReq>> {
    let mut out = Vec::new();
    for (n, line) in md.lines().enumerate() {
        let Some(rest) = line
            .strip_prefix("- [ ] ")
            .or_else(|| line.strip_prefix("- [x] "))
        else {
            continue;
        };
        let req_type = if rest.starts_with("INV-") {
            ReqType::Invariant
        } else if rest.starts_with("ISC-") {
            ReqType::Milestone
        } else {
            continue; // checkbox line but not a requirement
        };
        let parsed = rest.split_once(": ").and_then(|(id_region, tail)| {
            let text = tail.split_once(" | Verify:").map_or(tail, |(t, _)| t);
            let text = text.trim();
            // A trailing `)` marks an annotation; the `!id.contains(' ')` guard below
            // then rejects a second annotation or trailing junk in the remainder.
            let (id, annotation) = id_region
                .strip_suffix(')')
                .and_then(|head| head.rsplit_once(" ("))
                .map_or((id_region, None), |(id, ann)| (id, Some(ann)));
            let annotation_ok = annotation.is_none_or(|a| !a.is_empty());
            (!id.contains(' ') && !text.is_empty() && annotation_ok).then(|| ParsedReq {
                id: id.trim_end_matches(':').to_string(),
                req_type,
                text: text.to_string(),
                annotation: annotation.map(str::to_string),
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

/// Mark the named ids satisfied, rewriting only the `- [ ]` box of a line whose
/// ID region — annotation included, byte for byte — matches.
///
/// Per-line and anchored at the line start, so a `- [ ] <id>:` literal quoted in
/// prose or in a LOG bullet is left alone.
pub fn flip_checkboxes(prd: &str, ids: &[String]) -> String {
    // split_inclusive keeps each line's terminator, so the rebuild is byte-exact
    // apart from the flipped boxes — no trailing-newline or CRLF surprises.
    prd.split_inclusive('\n')
        .map(|line| match line.strip_prefix("- [ ] ") {
            Some(rest) if ids.iter().any(|id| id_region_matches(rest, id)) => {
                format!("- [x] {rest}")
            }
            _ => line.to_string(),
        })
        .collect()
}

/// Whether `rest` — a checkbox line's body — opens with `id` or `id (annotation)`
/// followed by the grammar's `": "`.
fn id_region_matches(rest: &str, id: &str) -> bool {
    let Some(after_id) = rest.strip_prefix(id) else {
        return false;
    };
    after_id.starts_with(": ")
        || after_id
            .strip_prefix(" (")
            .and_then(|a| a.split_once("): "))
            // Same restrictions parse_requirements enforces, so reader and writer
            // agree on which lines are annotated requirement lines.
            .is_some_and(|(annotation, _)| !annotation.is_empty() && !annotation.contains(')'))
}

/// Append a LOG entry at the true end of the file — LOG entries are always
/// the file's tail, so end-of-file append can never reverse their order.
/// Joins with a single newline: one-bullet entries form a tight list.
pub fn append_log(prd: &str, entry: &str) -> String {
    format!("{}\n{}\n", prd.trim_end(), entry.trim_end())
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
    fn parses_checked_unchecked_and_ignores_prose() {
        let md = "# Title\nprose here\n- [ ] ISC-A1: does a thing | Verify: Test: unit\n- [x] INV-B2: never breaks | Verify: Grep\n- [ ] plain checkbox, not a requirement\n";
        let reqs = parse_requirements(md).unwrap();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0], ParsedReq { id: "ISC-A1".into(), req_type: ReqType::Milestone, text: "does a thing".into(), annotation: None });
        assert_eq!(reqs[1], ParsedReq { id: "INV-B2".into(), req_type: ReqType::Invariant, text: "never breaks".into(), annotation: None });
    }

    // ISC-ANN-1: one optional annotation before the colon, on either prefix; None when absent.
    /// Same document with an annotation on the second requirement.
    const ANNOTATED_PRD: &str = "---\ntitle: t\nstatus: ACTIVE\nupdated: 2026-01-01\nverification_summary: \"old\"\nfailing_criteria: none\nlast_phase: ACT\n---\n\n# PRD\n\n- [ ] ISC-A1: first | Verify: test\n- [ ] ISC-A2 (blocking): second | Verify: test\n\n## LOG\n";

    #[test]
    fn parses_optional_annotation_on_both_prefixes() {
        let md = "- [ ] INV-D1 (blocking): never breaks | Verify: Grep\n- [x] ISC-A1 (deferred to v2): does a thing | Verify: Test\n- [ ] ISC-A2: plain | Verify: Test\n";
        let reqs = parse_requirements(md).unwrap();
        assert_eq!(
            reqs[0],
            ParsedReq { id: "INV-D1".into(), req_type: ReqType::Invariant, text: "never breaks".into(), annotation: Some("blocking".into()) }
        );
        assert_eq!(
            reqs[1],
            ParsedReq { id: "ISC-A1".into(), req_type: ReqType::Milestone, text: "does a thing".into(), annotation: Some("deferred to v2".into()) }
        );
        assert_eq!(reqs[2].annotation, None);
    }

    // ISC-ANN-2: the split lands before the text, so text parens are never examined.
    #[test]
    fn parens_in_text_stay_text() {
        let md = "- [ ] ISC-A1: holds when (and only when) x | Verify: Test\n";
        let reqs = parse_requirements(md).unwrap();
        assert_eq!(reqs[0].annotation, None);
        assert_eq!(reqs[0].text, "holds when (and only when) x");
    }

    // ISC-ANN-3: the `!id.contains(' ')` guard rejects both malformed forms.
    #[test]
    fn two_annotations_error_with_line_number_and_fallback() {
        let err = parse_requirements("ok\n- [ ] INV-D1 (a) (b): text | Verify: Grep\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("line 2"), "{err}");
        assert!(err.contains("req add"), "{err}");
    }

    #[test]
    fn unclosed_annotation_errors_with_line_number_and_fallback() {
        let err = parse_requirements("ok\n- [ ] INV-D1 (blocking: text | Verify: Grep\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("line 2"), "{err}");
        assert!(err.contains("req add"), "{err}");
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
    fn flip_checkboxes_flips_only_named_ids() {
        let out = flip_checkboxes(PRD, &["ISC-A1".into()]);
        assert!(out.contains("- [x] ISC-A1: first"));
        assert!(out.contains("- [ ] ISC-A2: second"));
    }

    #[test]
    fn parse_survives_flip_round_trip() {
        let before = parse_requirements(ANNOTATED_PRD).unwrap();
        assert_eq!(before[1].annotation.as_deref(), Some("blocking"));
        let flipped = flip_checkboxes(ANNOTATED_PRD, &["ISC-A1".into(), "ISC-A2".into()]);
        let after = parse_requirements(&flipped).unwrap();
        assert_eq!(before, after, "flip must not change what parse sees");
    }

    // ISC-ANN-4: the annotation is matched, not rewritten.
    #[test]
    fn flip_preserves_annotation_byte_for_byte() {
        let out = flip_checkboxes(ANNOTATED_PRD, &["ISC-A2".into()]);
        assert!(out.contains("- [x] ISC-A2 (blocking): second | Verify: test\n"), "{out}");
        assert!(out.contains("- [ ] ISC-A1: first"), "unnamed id must not flip: {out}");
    }

    // ISC-ANN-4: anchored per line, so the literal in prose and in a LOG bullet survives.
    #[test]
    fn flip_leaves_the_literal_outside_a_requirement_line_alone() {
        let md = "# PRD\n\n- [ ] ISC-A1: real | Verify: test\n\nProse mentioning - [ ] ISC-A1: inline.\n\n## LOG\n- **1** · flipped - [ ] ISC-A1: as recorded\n";
        let out = flip_checkboxes(md, &["ISC-A1".into()]);
        assert!(out.contains("- [x] ISC-A1: real"), "{out}");
        assert!(out.contains("Prose mentioning - [ ] ISC-A1: inline."), "{out}");
        assert!(out.contains("- **1** · flipped - [ ] ISC-A1: as recorded"), "{out}");
    }

    #[test]
    fn flip_ignores_a_prefix_of_a_longer_id() {
        let md = "- [ ] ISC-A1: a | Verify: t\n- [ ] ISC-A10: b | Verify: t\n";
        let out = flip_checkboxes(md, &["ISC-A1".into()]);
        assert!(out.contains("- [x] ISC-A1: a"), "{out}");
        assert!(out.contains("- [ ] ISC-A10: b"), "ISC-A1 must not match ISC-A10: {out}");
    }

    #[test]
    fn append_log_lands_after_last_entry() {
        let out = append_log(PRD, "### Iteration 3 — 2026-01-03\n- **Overall:** FAIL");
        let it2 = out.find("### Iteration 2").unwrap();
        let it3 = out.find("### Iteration 3").unwrap();
        assert!(it3 > it2, "new entry must follow the last existing one");
        assert!(out.ends_with("- **Overall:** FAIL\n"));
    }

    // ISC-LOG-6: bullet-list LOG joins tight (single newline), no blank line between entries.
    #[test]
    fn append_log_tight_joins_bullets_with_single_newline() {
        let prd = "# PRD\n\n## LOG\n- **1** · 2026-01-01 · SG-1 — a → ISC-1 satisfied\n- **2** · 2026-01-02 · SG-2 — b → ISC-2 satisfied\n";
        let out = append_log(prd, "- **3** · 2026-01-03 · SG-3 — c → ISC-3 satisfied");
        assert!(
            out.contains("→ ISC-2 satisfied\n- **3** · 2026-01-03"),
            "third bullet must be contiguous with the second: {out:?}"
        );
        assert!(!out.contains("satisfied\n\n- **3**"), "no blank line between bullets");
        assert!(out.ends_with("→ ISC-3 satisfied\n"));
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
