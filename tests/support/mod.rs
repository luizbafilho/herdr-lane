use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use lane_menu::lane::{Ctx, Lane};
use serde_json::json;
use tempfile::TempDir;

/// Logs each call's argv to `calls` and answers list queries from canned JSON
/// files beside it.
const FAKE_HERDR: &str = r#"#!/usr/bin/env bash
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$1 $2" in
  "worktree list") cat "$dir/worktree-list.json" ;;
  "workspace list") cat "$dir/workspace-list.json" ;;
  *) echo '{}' ;;
esac
"#;

/// A git repo that ran `lane init`, wired to a fake herdr.
pub struct Repo {
    _dir: TempDir,
    pub ctx: Ctx,
    herdr_dir: PathBuf,
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn lane(dir: &Path, args: &[&str]) {
    let output = Command::new("lane")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "lane {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

impl Repo {
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("README"), "hi\n").unwrap();
        git(&repo, &["add", "README"]);
        git(&repo, &["commit", "-qm", "init"]);
        lane(&repo, &["init"]);

        let herdr_dir = dir.path().join("herdr");
        fs::create_dir(&herdr_dir).unwrap();
        let herdr = herdr_dir.join("herdr");
        fs::write(&herdr, FAKE_HERDR).unwrap();
        fs::set_permissions(&herdr, fs::Permissions::from_mode(0o755)).unwrap();

        let made = Self {
            _dir: dir,
            ctx: Ctx {
                repo,
                current: None,
                herdr,
            },
            herdr_dir,
        };
        made.set_worktrees(&[]);
        made.set_workspaces(&[]);
        made
    }

    pub fn path(&self) -> &Path {
        &self.ctx.repo
    }

    pub fn lane_path(&self, name: &str) -> PathBuf {
        self.path().join(".lane/trees").join(name)
    }

    pub fn new_lane(&self, name: &str) {
        lane(self.path(), &["new", name]);
    }

    pub fn commit_in(&self, name: &str, file: &str) {
        let path = self.lane_path(name);
        fs::write(path.join(file), "change\n").unwrap();
        git(&path, &["add", file]);
        git(&path, &["commit", "-qm", file]);
    }

    pub fn lanes(&self) -> Vec<Lane> {
        self.ctx.lanes().unwrap()
    }

    pub fn find(&self, name: &str) -> Lane {
        self.lanes()
            .into_iter()
            .find(|lane| lane.name == name)
            .unwrap()
    }

    /// What `herdr worktree list` reports: the primary checkout and each
    /// lane, with the workspaces given as `(path, workspace id)` open on them.
    pub fn set_worktrees(&self, open: &[(&Path, &str)]) {
        let workspace = |path: &Path| {
            open.iter()
                .find(|(open, _)| *open == path)
                .map(|(_, id)| *id)
        };
        let mut worktrees = vec![json!({
            "path": self.path(),
            "is_linked_worktree": false,
            "open_workspace_id": workspace(self.path()),
        })];
        let ls = Command::new("lane")
            .args(["ls", "--json"])
            .current_dir(self.path())
            .output()
            .unwrap();
        let lanes: Vec<serde_json::Value> = serde_json::from_slice(&ls.stdout).unwrap();
        for lane in lanes {
            let path = PathBuf::from(lane["path"].as_str().unwrap());
            worktrees.push(json!({
                "path": path,
                "is_linked_worktree": true,
                "open_workspace_id": workspace(&path),
            }));
        }
        let reply = json!({ "result": { "worktrees": worktrees } });
        fs::write(self.herdr_dir.join("worktree-list.json"), reply.to_string()).unwrap();
    }

    /// What `herdr workspace list` reports, as `(workspace id, repo root, checkout path)`.
    pub fn set_workspaces(&self, workspaces: &[(&str, &Path, &Path)]) {
        let workspaces: Vec<_> = workspaces
            .iter()
            .map(|(id, repo, checkout)| {
                json!({
                    "workspace_id": id,
                    "worktree": {
                        "repo_root": repo,
                        "checkout_path": checkout,
                        "is_linked_worktree": checkout != repo,
                    },
                })
            })
            .chain([json!({ "workspace_id": "w-plain" })])
            .collect();
        let reply = json!({ "result": { "workspaces": workspaces } });
        fs::write(
            self.herdr_dir.join("workspace-list.json"),
            reply.to_string(),
        )
        .unwrap();
    }

    /// Every herdr call made so far, argv joined by spaces, minus the read-only lists.
    pub fn herdr_calls(&self) -> Vec<String> {
        fs::read_to_string(self.herdr_dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .filter(|call| !call.starts_with("worktree list") && *call != "workspace list")
            .map(str::to_owned)
            .collect()
    }

    /// A bare remote with `main` pushed, so `lane push` has somewhere to go.
    pub fn add_remote(&self) -> PathBuf {
        let remote = self.path().with_file_name("remote.git");
        git(
            self.path(),
            &["init", "-q", "--bare", remote.to_str().unwrap()],
        );
        git(
            self.path(),
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(self.path(), &["push", "-q", "origin", "main"]);
        remote
    }
}
