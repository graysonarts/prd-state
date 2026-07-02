mod decide;
mod end_iteration;
mod phase;
mod prd_md;
mod req;
mod state;
mod status;
mod subgoal;
mod sync;
mod verify;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Manage the `prd_work_loop` state.json — the tool, not the agent, owns the schema.
#[derive(Parser)]
#[command(name = "prd-state", version)]
struct Cli {
    /// PRD directory containing state.json (default: current directory)
    #[arg(short = 'C', long = "dir", global = true, default_value = ".")]
    dir: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a canonical state.json next to the PRD
    Init {
        /// Path to the PRD markdown file (relative to -C dir)
        prd_path: PathBuf,
    },
    /// Show iteration, phase, next subgoal, and the computed resume phase
    Status {
        /// Emit the summary as JSON
        #[arg(long)]
        json: bool,
    },
    /// Set the current phase; OBSERVE also captures `start_commit` from git HEAD
    Phase {
        #[arg(ignore_case = true)]
        phase: state::Phase,
    },
    /// Manually edit the requirements registry
    Req {
        #[command(subcommand)]
        cmd: ReqCmd,
    },
    /// Sync the registry from the invariant doc and the PRD's ISC lines
    Sync,
    /// Close out the iteration: registry, subgoal, stall, PRD writes, field reset
    EndIteration {
        /// Reflection text for the LOG entry
        #[arg(long)]
        reflection: Option<String>,
    },
    /// Record a verify result with its evidence
    Verify {
        /// Requirement id from the pre-flight checklist
        id: String,
        #[arg(ignore_case = true)]
        status: state::VerifyStatus,
        /// Citation proving the result (line, test, or search)
        evidence: String,
    },
    /// Mark a subgoal `in_progress` and derive its pre-flight checklist
    Decide {
        /// Subgoal id (e.g. SG-3)
        sg_id: String,
    },
    /// Edit the subgoal list
    Subgoal {
        #[command(subcommand)]
        cmd: SubgoalCmd,
    },
}

#[derive(Subcommand)]
enum SubgoalCmd {
    /// Add a pending subgoal
    Add {
        id: String,
        #[arg(long, ignore_case = true)]
        tier: state::Tier,
        #[arg(long, value_delimiter = ',', required = true)]
        artifacts: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        milestones: Vec<String>,
        #[arg(long)]
        desc: String,
    },
    /// Delete a subgoal
    Remove { id: String },
}

#[derive(Subcommand)]
enum ReqCmd {
    /// Register a requirement
    Add {
        id: String,
        #[arg(ignore_case = true)]
        req_type: state::ReqType,
        text: String,
    },
    /// Mark a milestone removed (invariants are deleted outright)
    Remove { id: String },
}

/// Artifact paths are repo-root-relative; fall back to the PRD dir outside git.
fn artifact_root(dir: &Path) -> PathBuf {
    Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || dir.to_path_buf(),
            |o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()),
        )
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init { prd_path } => {
            let created = state::init(&cli.dir.join(prd_path))?;
            println!("initialized {}", created.display());
        }
        Cmd::Phase { phase } => {
            println!("{}", phase::set_phase(&cli.dir, phase)?);
        }
        Cmd::Req { cmd } => match cmd {
            ReqCmd::Add { id, req_type, text } => {
                println!("{}", req::add(&cli.dir, &id, req_type, &text)?);
            }
            ReqCmd::Remove { id } => {
                println!("{}", req::remove(&cli.dir, &id)?);
            }
        },
        Cmd::Decide { sg_id } => {
            println!("{}", decide::run(&cli.dir, &sg_id)?);
        }
        Cmd::Subgoal { cmd } => match cmd {
            SubgoalCmd::Add { id, tier, artifacts, milestones, desc } => {
                println!("{}", subgoal::add(&cli.dir, &id, tier, artifacts, milestones, &desc)?);
            }
            SubgoalCmd::Remove { id } => {
                println!("{}", subgoal::remove(&cli.dir, &id)?);
            }
        },
        Cmd::EndIteration { reflection } => {
            println!("{}", end_iteration::run(&cli.dir, reflection.as_deref())?);
        }
        Cmd::Verify { id, status, evidence } => {
            println!("{}", verify::run(&cli.dir, &id, status, &evidence)?);
        }
        Cmd::Sync => {
            let doc = artifact_root(&cli.dir).join("docs/invariant_requirements.md");
            println!("{}", sync::run(&cli.dir, Some(&doc))?);
        }
        Cmd::Status { json } => {
            let state = state::load(&cli.dir)?;
            let summary = status::summary(&state, &artifact_root(&cli.dir));
            if json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                println!("{}", status::render(&summary));
            }
        }
    }
    Ok(())
}
