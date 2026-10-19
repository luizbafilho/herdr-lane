#[allow(dead_code, reason = "each test crate uses part of the shared fixture")]
mod support;

use std::{path::Path, process::Command};

use serde_json::json;
use support::Repo;

/// Runs `bin/lane-action` as herdr would for a workspace whose directory is
/// `cwd` and that herdr did not open as a worktree.
fn lane_action(repo: &Repo, cwd: &Path) -> bool {
    let context = json!({ "workspace_id": "w1", "workspace_cwd": cwd });
    Command::new("bash")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("bin/lane-action"))
        .env("HERDR_BIN_PATH", &repo.ctx.herdr)
        .env("HERDR_PLUGIN_CONTEXT_JSON", context.to_string())
        .status()
        .unwrap()
        .success()
}

fn opened_menu(repo: &Path, checkout: &Path) -> String {
    format!(
        "plugin pane open --plugin lane --entrypoint menu --cwd {repo} --env LANE_REPO_ROOT={repo} --env LANE_CHECKOUT={checkout} --focus",
        repo = repo.display(),
        checkout = checkout.display(),
    )
}

#[test]
fn opens_the_menu_from_a_plain_workspace_on_the_primary_checkout() {
    let repo = Repo::new();

    assert!(lane_action(&repo, repo.path()));

    assert_eq!(repo.herdr_calls(), [opened_menu(repo.path(), repo.path())]);
}

#[test]
fn opens_the_menu_from_a_lane_with_the_lane_as_the_checkout() {
    let repo = Repo::new();
    repo.new_lane("feat");

    assert!(lane_action(&repo, &repo.lane_path("feat")));

    assert_eq!(
        repo.herdr_calls(),
        [opened_menu(repo.path(), &repo.lane_path("feat"))]
    );
}

#[test]
fn notifies_when_the_workspace_is_not_in_a_git_repo() {
    let repo = Repo::new();

    assert!(!lane_action(&repo, repo.path().parent().unwrap()));

    assert_eq!(
        repo.herdr_calls(),
        ["notification show lane --body this workspace is not a git repository"]
    );
}
