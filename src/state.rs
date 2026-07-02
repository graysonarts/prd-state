//! Canonical state.json schema and file I/O. Strict: unknown fields are errors.
//! Home of the requirement Registry: every Milestone status transition and
//! traversal goes through its interface.

use crate::prd_md::ParsedReq;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const STATE_FILE: &str = "state.json";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "UPPERCASE")]
#[value(rename_all = "UPPER")]
pub enum Phase {
    Observe,
    Orient,
    Decide,
    Act,
    Verify,
    Update,
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Phase::Observe => "OBSERVE",
            Phase::Orient => "ORIENT",
            Phase::Decide => "DECIDE",
            Phase::Act => "ACT",
            Phase::Verify => "VERIFY",
            Phase::Update => "UPDATE",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
#[value(rename_all = "lower")]
pub enum Tier {
    Trivial,
    Standard,
    Complex,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Tier::Trivial => "trivial",
            Tier::Standard => "standard",
            Tier::Complex => "complex",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
#[value(rename_all = "lower")]
pub enum ReqType {
    Invariant,
    Milestone,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReqStatus {
    Active,
    Satisfied,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubgoalStatus {
    Pending,
    InProgress,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "UPPERCASE")]
#[value(rename_all = "UPPER")]
pub enum VerifyStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentAction {
    pub artifacts: Vec<String>,
    pub tier: Tier,
    pub description: String,
    pub applicable_milestones: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistItem {
    pub id: String,
    #[serde(rename = "type")]
    pub req_type: ReqType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyResult {
    pub id: String,
    pub status: VerifyStatus,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subgoal {
    pub id: String,
    pub artifacts: Vec<String>,
    pub tier: Tier,
    pub description: String,
    pub milestones: Vec<String>,
    pub status: SubgoalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    #[serde(rename = "type")]
    pub req_type: ReqType,
    /// Milestones: active | satisfied | removed. Invariants carry no status.
    /// Enforced by Registry's `TryFrom` on load and its mutation methods.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<ReqStatus>,
    pub text: String,
}

/// The requirement registry. The vec is private: reads go through Deref,
/// every mutation through a method, so the Milestone/Invariant status rule
/// holds by construction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(try_from = "Vec<Requirement>")]
pub struct Registry(Vec<Requirement>);

impl TryFrom<Vec<Requirement>> for Registry {
    type Error = String;

    fn try_from(reqs: Vec<Requirement>) -> Result<Self, String> {
        for r in &reqs {
            match (r.req_type, r.status) {
                (ReqType::Invariant, Some(_)) => {
                    return Err(format!("invariant {} must not carry a status", r.id))
                }
                (ReqType::Milestone, None) => {
                    return Err(format!("milestone {} must carry a status", r.id))
                }
                _ => {}
            }
        }
        Ok(Registry(reqs))
    }
}

impl std::ops::Deref for Registry {
    type Target = [Requirement];
    fn deref(&self) -> &[Requirement] {
        &self.0
    }
}

impl Registry {
    /// Register a requirement: Milestones start active, Invariants carry no status.
    pub fn add(&mut self, id: &str, req_type: ReqType, text: &str) -> Result<()> {
        if self.find(id).is_some() {
            bail!("requirement {id} already registered");
        }
        self.0.push(Requirement {
            id: id.to_string(),
            req_type,
            status: matches!(req_type, ReqType::Milestone).then_some(ReqStatus::Active),
            text: text.to_string(),
        });
        Ok(())
    }

    /// Milestones stay in the registry as removed history; invariants have no
    /// status, so a remove deletes the entry. Returns the removed entry's type.
    pub fn remove(&mut self, id: &str) -> Result<ReqType> {
        let Some(idx) = self.0.iter().position(|r| r.id == id) else {
            bail!("requirement {id} not found");
        };
        let req_type = self.0[idx].req_type;
        match req_type {
            ReqType::Milestone => self.0[idx].status = Some(ReqStatus::Removed),
            ReqType::Invariant => {
                self.0.remove(idx);
            }
        }
        Ok(req_type)
    }

    pub fn mark_satisfied(&mut self, id: &str) -> Result<()> {
        let req = self
            .0
            .iter_mut()
            .find(|r| r.id == id)
            .with_context(|| format!("milestone {id} not in registry"))?;
        if req.req_type != ReqType::Milestone {
            bail!("{id} is an invariant; only milestones can be satisfied");
        }
        req.status = Some(ReqStatus::Satisfied);
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&Requirement> {
        self.0.iter().find(|r| r.id == id)
    }

    pub fn invariants(&self) -> impl Iterator<Item = &Requirement> {
        self.0.iter().filter(|r| r.req_type == ReqType::Invariant)
    }

    pub fn pending_milestones(&self) -> usize {
        self.0
            .iter()
            .filter(|r| r.req_type == ReqType::Milestone && r.status == Some(ReqStatus::Active))
            .count()
    }

    /// Diff parsed requirements into the registry. The source is authoritative
    /// for wording; Milestones absent from it are removed, Invariants never are.
    pub fn upsert_from_parsed(&mut self, parsed: &[ParsedReq]) -> SyncReport {
        let mut report = SyncReport::default();
        for p in parsed {
            match self.0.iter_mut().find(|r| r.id == p.id) {
                None => {
                    self.add(&p.id, p.req_type, &p.text).expect("id checked absent");
                    match p.req_type {
                        ReqType::Invariant => report.added_invariants += 1,
                        ReqType::Milestone => report.added_milestones += 1,
                    }
                }
                Some(existing) => {
                    existing.text.clone_from(&p.text);
                    if existing.status == Some(ReqStatus::Removed) {
                        existing.status = Some(ReqStatus::Active); // reappeared in PRD
                        report.added_milestones += 1;
                    } else {
                        report.unchanged += 1;
                    }
                }
            }
        }
        for r in &mut self.0 {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub iteration: u32,
    pub current_phase: Option<Phase>,
    pub start_commit: Option<String>,
    pub prd_path: String,
    pub current_action: Option<CurrentAction>,
    pub pre_flight_checklist: Vec<ChecklistItem>,
    pub verify_results: Vec<VerifyResult>,
    pub stall_count: u32,
    pub subgoals: Vec<Subgoal>,
    pub requirements: Registry,
}

impl State {
    pub fn new(prd_filename: &str) -> Self {
        State {
            iteration: 0,
            current_phase: None,
            start_commit: None,
            prd_path: prd_filename.to_string(),
            current_action: None,
            pre_flight_checklist: Vec::new(),
            verify_results: Vec::new(),
            stall_count: 0,
            subgoals: Vec::new(),
            requirements: Registry::default(),
        }
    }
}

/// Load and strictly validate `state.json` from the PRD directory.
pub fn load(dir: &Path) -> Result<State> {
    let path = dir.join(STATE_FILE);
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("invalid state file {} (schema is strict; unknown or malformed fields are errors)", path.display()))
}

/// Atomically write `state.json`: temp file in the same dir, then rename.
pub fn save(dir: &Path, state: &State) -> Result<()> {
    let tmp = dir.join(format!("{STATE_FILE}.tmp"));
    let path = dir.join(STATE_FILE);
    let json = serde_json::to_string_pretty(state)?;
    fs::write(&tmp, json.as_bytes())
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, &path)
        .with_context(|| format!("cannot rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Create a fresh canonical state.json next to the PRD. Refuses to overwrite.
pub fn init(prd_path: &Path) -> Result<PathBuf> {
    if !prd_path.is_file() {
        bail!("PRD not found: {}", prd_path.display());
    }
    let dir = prd_path.parent().context("PRD path has no parent directory")?;
    let state_path = dir.join(STATE_FILE);
    if state_path.exists() {
        bail!("{} already exists; refusing to overwrite", state_path.display());
    }
    let filename = prd_path
        .file_name()
        .context("PRD path has no filename")?
        .to_string_lossy();
    save(dir, &State::new(&filename))?;
    Ok(state_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn prd_dir_with_prd() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let prd = dir.path().join("PRD-test.md");
        fs::write(&prd, "# PRD\n").unwrap();
        (dir, prd)
    }

    #[test]
    fn init_creates_canonical_state() {
        let (dir, prd) = prd_dir_with_prd();
        let state_path = init(&prd).unwrap();
        assert_eq!(state_path, dir.path().join("state.json"));
        let state = load(dir.path()).unwrap();
        assert_eq!(state.iteration, 0);
        assert_eq!(state.current_phase, None);
        assert_eq!(state.prd_path, "PRD-test.md");
        assert_eq!(state.stall_count, 0);
        assert!(state.subgoals.is_empty());
        assert!(state.requirements.is_empty());
    }

    #[test]
    fn init_rejects_missing_prd() {
        let dir = TempDir::new().unwrap();
        let err = init(&dir.path().join("nope.md")).unwrap_err();
        assert!(err.to_string().contains("PRD not found"));
    }

    #[test]
    fn init_refuses_overwrite() {
        let (dir, prd) = prd_dir_with_prd();
        init(&prd).unwrap();
        let err = init(&prd).unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
        drop(dir);
    }

    #[test]
    fn load_rejects_unknown_keys() {
        let dir = TempDir::new().unwrap();
        // Real drift observed in the wild: requirements_registry instead of requirements.
        fs::write(
            dir.path().join("state.json"),
            r#"{"iteration":0,"current_phase":null,"start_commit":null,"prd_path":"p.md",
                "current_action":null,"pre_flight_checklist":[],"verify_results":[],
                "stall_count":0,"subgoals":[],"requirements":[],"requirements_registry":[]}"#,
        )
        .unwrap();
        let err = load(dir.path()).unwrap_err();
        let chain = format!("{err:#}");
        assert!(chain.contains("unknown field"), "error was: {chain}");
        assert!(chain.contains("schema is strict"), "error was: {chain}");
    }

    #[test]
    fn load_rejects_bad_enum_value() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("state.json"),
            r#"{"iteration":0,"current_phase":"NAPPING","start_commit":null,"prd_path":"p.md",
                "current_action":null,"pre_flight_checklist":[],"verify_results":[],
                "stall_count":0,"subgoals":[],"requirements":[]}"#,
        )
        .unwrap();
        assert!(load(dir.path()).is_err());
    }

    #[test]
    fn save_load_roundtrip_leaves_no_tmp() {
        let dir = TempDir::new().unwrap();
        let mut state = State::new("p.md");
        state.iteration = 7;
        state.current_phase = Some(Phase::Verify);
        state.requirements.add("ISC-X1", ReqType::Milestone, "does the thing").unwrap();
        save(dir.path(), &state).unwrap();
        assert!(!dir.path().join("state.json.tmp").exists());
        let loaded = load(dir.path()).unwrap();
        assert_eq!(loaded.iteration, 7);
        assert_eq!(loaded.current_phase, Some(Phase::Verify));
        assert_eq!(loaded.requirements[0].id, "ISC-X1");
        assert_eq!(loaded.requirements[0].status, Some(ReqStatus::Active));
    }

    #[test]
    fn invariants_serialize_without_status_key() {
        let mut state = State::new("p.md");
        state.requirements.add("INV-A1", ReqType::Invariant, "always").unwrap();
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("\"status\":null"));
    }

    #[test]
    fn load_rejects_invariant_with_status() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("state.json"),
            r#"{"iteration":0,"current_phase":null,"start_commit":null,"prd_path":"p.md",
                "current_action":null,"pre_flight_checklist":[],"verify_results":[],
                "stall_count":0,"subgoals":[],
                "requirements":[{"id":"INV-A1","type":"invariant","status":"active","text":"x"}]}"#,
        )
        .unwrap();
        let err = format!("{:#}", load(dir.path()).unwrap_err());
        assert!(err.contains("INV-A1 must not carry a status"), "error was: {err}");
    }

    #[test]
    fn load_rejects_milestone_without_status() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("state.json"),
            r#"{"iteration":0,"current_phase":null,"start_commit":null,"prd_path":"p.md",
                "current_action":null,"pre_flight_checklist":[],"verify_results":[],
                "stall_count":0,"subgoals":[],
                "requirements":[{"id":"ISC-A1","type":"milestone","text":"x"}]}"#,
        )
        .unwrap();
        let err = format!("{:#}", load(dir.path()).unwrap_err());
        assert!(err.contains("ISC-A1 must carry a status"), "error was: {err}");
    }

    #[test]
    fn mark_satisfied_rejects_invariants() {
        let mut reg = Registry::default();
        reg.add("INV-A1", ReqType::Invariant, "always").unwrap();
        let err = reg.mark_satisfied("INV-A1").unwrap_err();
        assert!(err.to_string().contains("only milestones"));
    }

    #[test]
    fn upsert_registers_invariants_deduplicating() {
        let mut reg = Registry::default();
        reg.add("INV-A1", ReqType::Invariant, "old wording").unwrap();
        let parsed = vec![
            ParsedReq { id: "INV-A1".into(), req_type: ReqType::Invariant, text: "new wording".into() },
            ParsedReq { id: "INV-A2".into(), req_type: ReqType::Invariant, text: "b".into() },
            ParsedReq { id: "INV-A3".into(), req_type: ReqType::Invariant, text: "c".into() },
        ];
        let report = reg.upsert_from_parsed(&parsed);
        assert_eq!(report.added_invariants, 2);
        assert_eq!(report.unchanged, 1);
        assert_eq!(reg.len(), 3);
        assert_eq!(reg[0].text, "new wording"); // doc authoritative
    }

    #[test]
    fn upsert_diffs_milestones_preserving_satisfied() {
        let mut reg = Registry::default();
        reg.add("ISC-A", ReqType::Milestone, "a").unwrap();
        reg.mark_satisfied("ISC-A").unwrap();
        reg.add("ISC-B", ReqType::Milestone, "b").unwrap();
        // PRD now has A and C; B is gone.
        let parsed = vec![
            ParsedReq { id: "ISC-A".into(), req_type: ReqType::Milestone, text: "a".into() },
            ParsedReq { id: "ISC-C".into(), req_type: ReqType::Milestone, text: "c".into() },
        ];
        let report = reg.upsert_from_parsed(&parsed);
        assert_eq!(report.added_milestones, 1);
        assert_eq!(report.removed, 1);
        assert_eq!(report.unchanged, 1);
        let get = |id: &str| reg.find(id).unwrap().status;
        assert_eq!(get("ISC-A"), Some(ReqStatus::Satisfied));
        assert_eq!(get("ISC-B"), Some(ReqStatus::Removed));
        assert_eq!(get("ISC-C"), Some(ReqStatus::Active));
    }

    #[test]
    fn upsert_reactivates_removed_milestone_back_in_prd() {
        let mut reg = Registry::default();
        reg.add("ISC-A", ReqType::Milestone, "a").unwrap();
        reg.remove("ISC-A").unwrap();
        let parsed = vec![ParsedReq { id: "ISC-A".into(), req_type: ReqType::Milestone, text: "a".into() }];
        reg.upsert_from_parsed(&parsed);
        assert_eq!(reg[0].status, Some(ReqStatus::Active));
    }
}
