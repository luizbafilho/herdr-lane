#[allow(dead_code, reason = "each test crate uses part of the shared fixture")]
mod support;

use std::{
    path::Path,
    process::{Command, Output},
};

use support::Repo;

fn lane_menu(repo: &Repo, current: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lane-menu"));
    command
        .args(args)
        .env("LANE_REPO_ROOT", repo.path())
        .env("HERDR_BIN_PATH", &repo.ctx.herdr)
        .env_remove("LANE_CHECKOUT");
    if let Some(current) = current {
        command.env("LANE_CHECKOUT", current);
    }
    command.output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_verb_and_a_lane_name_run_that_handler() {
    let repo = Repo::new();
    repo.new_lane("fix-login");
    repo.commit_in("fix-login", "fix.txt");

    let output = lane_menu(&repo, None, &["merge", "fix-login"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(String::from_utf8_lossy(&output.stdout).contains("fast-forwarded main"));
    assert!(repo.lanes().is_empty());
}

#[test]
fn a_key_works_like_its_verb_and_defaults_to_the_current_lane() {
    let repo = Repo::new();
    repo.new_lane("here");
    repo.new_lane("there");

    let output = lane_menu(&repo, Some(&repo.lane_path("here")), &["r"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let left: Vec<_> = repo.lanes().into_iter().map(|lane| lane.name).collect();
    assert_eq!(left, ["there"]);
}

#[test]
fn new_takes_a_name_and_an_optional_base() {
    let repo = Repo::new();

    let output = lane_menu(&repo, None, &["new", "spike", "main"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(repo.find("spike").branch, "spike");
    assert_eq!(repo.herdr_calls().len(), 1);
}

#[test]
fn rm_fails_on_refusal_without_prompting() {
    let repo = Repo::new();
    repo.new_lane("wip");
    std::fs::write(repo.lane_path("wip").join("README"), "edited\n").unwrap();

    let output = lane_menu(&repo, None, &["rm", "wip"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("uncommitted"),
        "{}",
        stderr(&output)
    );
    assert_eq!(repo.find("wip").name, "wip");
}

#[test]
fn open_refuses_a_lane_that_already_has_a_workspace() {
    let repo = Repo::new();
    repo.new_lane("busy");
    repo.set_worktrees(&[(&repo.lane_path("busy"), "w-busy")]);

    let output = lane_menu(&repo, None, &["open", "busy"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("already open"),
        "{}",
        stderr(&output)
    );
    assert!(repo.herdr_calls().is_empty());
}

#[test]
fn unknown_lanes_and_verbs_fail() {
    let repo = Repo::new();

    let missing = lane_menu(&repo, None, &["push", "nope"]);
    let unknown = lane_menu(&repo, None, &["frobnicate"]);

    assert_eq!(missing.status.code(), Some(1));
    assert!(stderr(&missing).contains("no such lane: nope"));
    assert_eq!(unknown.status.code(), Some(2));
}
