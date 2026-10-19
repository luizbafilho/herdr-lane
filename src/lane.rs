use std::{
    collections::HashMap,
    env,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::Deserialize;

/// Where the menu runs: the repo it manages, the checkout it was opened from,
/// and the herdr binary to talk to.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub repo: PathBuf,
    pub current: Option<PathBuf>,
    pub herdr: PathBuf,
}

/// One lane from `lane ls --json`, joined with the herdr workspace it is open in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Lane {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
    pub state: String,
    pub dirty: bool,
    pub pending_notes: u32,
    #[serde(skip)]
    pub workspace: Option<String>,
}

/// A command that did not succeed, with everything it printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Error(String),
    /// `lane rm` kept the lane; `--force` would discard it.
    Refused(String),
}

impl Failure {
    pub fn output(&self) -> &str {
        match self {
            Self::Error(output) | Self::Refused(output) => output,
        }
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Error(error.to_string())
    }
}

#[derive(Deserialize)]
struct Reply<T> {
    result: T,
}

#[derive(Deserialize)]
struct WorktreeList {
    worktrees: Vec<Worktree>,
}

#[derive(Deserialize)]
struct Worktree {
    path: PathBuf,
    open_workspace_id: Option<String>,
    is_linked_worktree: bool,
}

#[derive(Deserialize)]
struct WorkspaceList {
    workspaces: Vec<Workspace>,
}

#[derive(Deserialize)]
struct Workspace {
    workspace_id: String,
    worktree: Option<WorkspaceWorktree>,
}

#[derive(Deserialize)]
struct WorkspaceWorktree {
    checkout_path: PathBuf,
    repo_root: PathBuf,
    is_linked_worktree: bool,
}

impl Ctx {
    /// Reads `LANE_REPO_ROOT` (required), `LANE_CHECKOUT` and `HERDR_BIN_PATH`.
    pub fn from_env() -> Result<Self, String> {
        let repo = env::var_os("LANE_REPO_ROOT").ok_or("LANE_REPO_ROOT is not set")?;
        Ok(Self {
            repo: repo.into(),
            current: env::var_os("LANE_CHECKOUT").map(PathBuf::from),
            herdr: env::var_os("HERDR_BIN_PATH").map_or_else(|| "herdr".into(), PathBuf::from),
        })
    }

    pub fn is_current(&self, lane: &Lane) -> bool {
        self.hosts_popup(&lane.path)
    }

    /// The popup belongs to the workspace on `LANE_CHECKOUT`.
    pub fn hosts_popup(&self, checkout: &Path) -> bool {
        self.current.as_deref() == Some(checkout)
    }

    pub fn repo_name(&self) -> String {
        self.repo
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    }

    pub fn lane<const N: usize>(&self, args: [&str; N]) -> Command {
        let mut command = Command::new("lane");
        command.args(args).current_dir(&self.repo);
        command
    }

    /// Runs one herdr CLI call and returns its JSON reply.
    pub fn herdr(&self, args: &[&str]) -> Result<String, Failure> {
        let output = Command::new(&self.herdr)
            .args(args)
            .current_dir(&self.repo)
            .stdin(Stdio::null())
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if output.status.success() {
            Ok(stdout)
        } else {
            Err(Failure::Error(
                stdout + &String::from_utf8_lossy(&output.stderr),
            ))
        }
    }

    /// Closes the workspace `id` open on `checkout`. Closing the popup's own
    /// workspace kills the popup, so that close focuses the primary first and
    /// must be the caller's last herdr call.
    pub fn close_workspace(&self, id: Option<&str>, checkout: &Path) -> Result<(), Failure> {
        let Some(id) = id else { return Ok(()) };
        if self.hosts_popup(checkout)
            && let Some(primary) = self.primary_workspace()?
        {
            self.herdr(&["workspace", "focus", &primary])?;
        }
        self.herdr(&["workspace", "close", id]).map(drop)
    }

    pub fn open_workspace(&self, path: &Path, label: &str) -> Result<(), Failure> {
        self.herdr(&[
            "worktree",
            "open",
            "--cwd",
            &self.repo.to_string_lossy(),
            "--path",
            &path.to_string_lossy(),
            "--label",
            label,
            "--focus",
        ])
        .map(drop)
    }

    fn worktrees(&self) -> Result<Vec<Worktree>, Failure> {
        let json = self.herdr(&["worktree", "list", "--cwd", &self.repo.to_string_lossy()])?;
        Ok(parse::<Reply<WorktreeList>>("herdr worktree list", &json)?
            .result
            .worktrees)
    }

    /// The workspace open on the repo's primary checkout, if any.
    pub fn primary_workspace(&self) -> Result<Option<String>, Failure> {
        Ok(self
            .worktrees()?
            .into_iter()
            .find(|worktree| !worktree.is_linked_worktree)
            .and_then(|worktree| worktree.open_workspace_id))
    }

    /// Workspaces on this repo's linked worktrees whose checkout no longer
    /// exists, as `(workspace id, checkout path)`.
    pub fn stale_workspaces(&self) -> Result<Vec<(String, PathBuf)>, Failure> {
        let json = self.herdr(&["workspace", "list"])?;
        let list = parse::<Reply<WorkspaceList>>("herdr workspace list", &json)?;
        Ok(list
            .result
            .workspaces
            .into_iter()
            .filter_map(|workspace| {
                let worktree = workspace.worktree?;
                (worktree.repo_root == self.repo
                    && worktree.is_linked_worktree
                    && !worktree.checkout_path.exists())
                .then_some((workspace.workspace_id, worktree.checkout_path))
            })
            .collect())
    }

    /// Every lane in the repo, each with the herdr workspace open on it.
    pub fn lanes(&self) -> Result<Vec<Lane>, Failure> {
        let output = self.lane(["ls", "--json"]).stdin(Stdio::null()).output()?;
        if !output.status.success() {
            return Err(Failure::Error(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        let mut lanes: Vec<Lane> =
            parse("lane ls --json", &String::from_utf8_lossy(&output.stdout))?;
        let mut open: HashMap<PathBuf, String> = self
            .worktrees()?
            .into_iter()
            .filter_map(|worktree| Some((worktree.path, worktree.open_workspace_id?)))
            .collect();
        for lane in &mut lanes {
            lane.workspace = open.remove(&lane.path);
        }
        Ok(lanes)
    }

    /// The last five commits on a lane, one line each.
    pub fn recent_commits(&self, lane: &Lane) -> Vec<String> {
        Command::new("git")
            .arg("-C")
            .arg(&lane.path)
            .args(["log", "--oneline", "-5"])
            .stdin(Stdio::null())
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn parse<T: for<'de> Deserialize<'de>>(source: &str, json: &str) -> Result<T, Failure> {
    serde_json::from_str(json)
        .map_err(|error| Failure::Error(format!("could not read {source}: {error}\n{json}")))
}

/// Runs `command` with stdout and stderr merged into one stream, in the order a
/// terminal would show them.
pub fn run(mut command: Command) -> Result<String, Failure> {
    let (mut reader, writer) = io::pipe()?;
    command
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer);
    let mut child = command.spawn()?;
    // The command holds the pipe's write ends; the read below only ends once
    // every write end is closed.
    drop(command);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    let output = String::from_utf8_lossy(&bytes).into_owned();
    if child.wait()?.success() {
        Ok(output)
    } else {
        Err(Failure::Error(output))
    }
}
