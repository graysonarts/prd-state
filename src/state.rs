//! Canonical state.json schema and file I/O. Strict: unknown fields are errors.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
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
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<ReqStatus>,
    pub text: String,
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
    pub requirements: Vec<Requirement>,
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
            requirements: Vec::new(),
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
        state.requirements.push(Requirement {
            id: "ISC-X1".into(),
            req_type: ReqType::Milestone,
            status: Some(ReqStatus::Active),
            text: "does the thing".into(),
        });
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
        state.requirements.push(Requirement {
            id: "INV-A1".into(),
            req_type: ReqType::Invariant,
            status: None,
            text: "always".into(),
        });
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("\"status\":null"));
    }
}
