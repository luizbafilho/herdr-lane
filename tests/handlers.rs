mod support;

use lane_menu::{
    lane::{Ctx, Failure, Lane},
    verb::{self, Handled, Target},
};
use support::{Repo, git};

fn on_lane(word: &str, ctx: &Ctx, lane: &Lane) -> Handled {
    match verb::find(word).unwrap().target {
        Target::AnyLane(handler) | Target::ClosedLane(handler) => handler(ctx, lane),
        Target::None(_) | Target::NewLane(_) => panic!("{word} does not act on a lane"),
    }
}

fn names(repo: &Repo) -> Vec<String> {
    repo.lanes().into_iter().map(|lane| lane.name).collect()
}

#[test]
fn lanes_carry_the_workspace_open_on_them() {
    let repo = Repo::new();
    repo.new_lane("feat-a");
    repo.new_lane("feat-b");
    repo.set_worktrees(&[(&repo.lane_path("feat-a"), "w-a")]);

    let lanes = repo.lanes();

    assert_eq!(lanes.len(), 2);
    let a = &lanes[0];
    assert_eq!(
        (a.name.as_str(), a.branch.as_str(), a.state.as_str()),
        ("feat-a", "feat-a", "open")
    );
    assert_eq!(a.path, repo.lane_path("feat-a"));
    assert_eq!(a.workspace.as_deref(), Some("w-a"));
    assert_eq!(lanes[1].workspace, None);
}

#[test]
fn new_creates_the_lane_and_opens_it_focused() {
    let repo = Repo::new();
    let Target::NewLane(create) = verb::find("new").unwrap().target else {
        panic!("new takes a name");
    };

    create(&repo.ctx, "fix-login", None).unwrap();

    assert_eq!(names(&repo), ["fix-login"]);
    let path = repo.lane_path("fix-login");
    assert_eq!(
        repo.herdr_calls(),
        [format!(
            "worktree open --cwd {} --path {} --label fix-login --focus",
            repo.path().display(),
            path.display()
        )]
    );
}

#[test]
fn new_branches_from_the_given_base() {
    let repo = Repo::new();
    let init = git(repo.path(), &["rev-parse", "HEAD"]);
    git(repo.path(), &["tag", "v1"]);
    git(
        repo.path(),
        &["commit", "-q", "--allow-empty", "-m", "later"],
    );
    let Target::NewLane(create) = verb::find("n").unwrap().target else {
        panic!("new takes a name");
    };

    create(&repo.ctx, "hotfix", Some("v1")).unwrap();

    assert_eq!(git(&repo.lane_path("hotfix"), &["rev-parse", "HEAD"]), init);
}

#[test]
fn new_reports_lanes_refusal_without_opening_anything() {
    let repo = Repo::new();
    repo.new_lane("taken");
    let Target::NewLane(create) = verb::find("new").unwrap().target else {
        panic!("new takes a name");
    };

    let failure = create(&repo.ctx, "taken", None).unwrap_err();

    assert!(matches!(failure, Failure::Error(_)));
    assert!(!failure.output().is_empty());
    assert!(repo.herdr_calls().is_empty());
}

#[test]
fn open_opens_a_lane_in_a_focused_workspace() {
    let repo = Repo::new();
    repo.new_lane("agent-made");

    on_lane("open", &repo.ctx, &repo.find("agent-made")).unwrap();

    assert_eq!(
        repo.herdr_calls(),
        [format!(
            "worktree open --cwd {} --path {} --label agent-made --focus",
            repo.path().display(),
            repo.lane_path("agent-made").display()
        )]
    );
}

#[test]
fn merge_lands_the_lane_closes_its_workspace_and_focuses_the_primary() {
    let repo = Repo::new();
    repo.new_lane("feat");
    repo.commit_in("feat", "feature.txt");
    repo.set_worktrees(&[(repo.path(), "w-main"), (&repo.lane_path("feat"), "w-feat")]);
    let lane = repo.find("feat");

    on_lane("merge", &repo.ctx, &lane).unwrap();

    assert!(names(&repo).is_empty());
    assert!(
        repo.path().join("feature.txt").exists(),
        "main has the lane's commit"
    );
    assert_eq!(
        repo.herdr_calls(),
        ["workspace close w-feat", "workspace focus w-main"]
    );
}

#[test]
fn merge_of_a_closed_lane_only_focuses_the_primary() {
    let repo = Repo::new();
    repo.new_lane("feat");
    repo.commit_in("feat", "feature.txt");
    repo.set_worktrees(&[(repo.path(), "w-main")]);

    on_lane("m", &repo.ctx, &repo.find("feat")).unwrap();

    assert_eq!(repo.herdr_calls(), ["workspace focus w-main"]);
}

#[test]
fn rm_refuses_a_dirty_lane_and_leaves_it_and_its_workspace() {
    let repo = Repo::new();
    repo.new_lane("wip");
    std::fs::write(repo.lane_path("wip").join("README"), "edited\n").unwrap();
    repo.set_worktrees(&[(&repo.lane_path("wip"), "w-wip")]);

    let failure = on_lane("rm", &repo.ctx, &repo.find("wip")).unwrap_err();

    assert!(matches!(failure, Failure::Refused(_)), "{failure:?}");
    assert!(
        failure.output().contains("uncommitted"),
        "{}",
        failure.output()
    );
    assert_eq!(names(&repo), ["wip"]);
    assert!(repo.herdr_calls().is_empty());
}

#[test]
fn rm_force_discards_a_refused_lane_and_closes_its_workspace() {
    let repo = Repo::new();
    repo.new_lane("wip");
    std::fs::write(repo.lane_path("wip").join("README"), "edited\n").unwrap();
    repo.set_worktrees(&[(&repo.lane_path("wip"), "w-wip")]);

    verb::rm_force(&repo.ctx, &repo.find("wip")).unwrap();

    assert!(names(&repo).is_empty());
    assert_eq!(repo.herdr_calls(), ["workspace close w-wip"]);
}

#[test]
fn rm_removes_a_clean_lane_and_closes_its_workspace() {
    let repo = Repo::new();
    repo.new_lane("done");
    repo.set_worktrees(&[(&repo.lane_path("done"), "w-done")]);

    on_lane("rm", &repo.ctx, &repo.find("done")).unwrap();

    assert!(names(&repo).is_empty());
    assert_eq!(repo.herdr_calls(), ["workspace close w-done"]);
}

#[test]
fn push_publishes_the_lane_branch() {
    let repo = Repo::new();
    let remote = repo.add_remote();
    repo.new_lane("review-me");
    repo.commit_in("review-me", "change.txt");

    on_lane("push", &repo.ctx, &repo.find("review-me")).unwrap();

    assert_eq!(
        git(&remote, &["rev-parse", "review-me"]),
        git(&repo.lane_path("review-me"), &["rev-parse", "HEAD"])
    );
    assert!(repo.herdr_calls().is_empty());
}

#[test]
fn prune_closes_only_this_repos_workspaces_whose_checkout_is_gone() {
    let repo = Repo::new();
    repo.add_remote();
    repo.new_lane("landed");
    repo.commit_in("landed", "landed.txt");
    git(repo.path(), &["merge", "-q", "landed"]);
    on_lane("push", &repo.ctx, &repo.find("landed")).unwrap();
    repo.new_lane("ongoing");
    assert_eq!(repo.find("landed").state, "landed");
    let elsewhere = repo.path().with_file_name("elsewhere");
    repo.set_workspaces(&[
        ("w-main", repo.path(), repo.path()),
        ("w-landed", repo.path(), &repo.lane_path("landed")),
        ("w-ongoing", repo.path(), &repo.lane_path("ongoing")),
        ("w-elsewhere", &elsewhere, &elsewhere.join("gone")),
    ]);
    let Target::None(prune) = verb::find("prune").unwrap().target else {
        panic!("prune takes no lane");
    };

    prune(&repo.ctx).unwrap();

    assert_eq!(names(&repo), ["ongoing"]);
    assert_eq!(repo.herdr_calls(), ["workspace close w-landed"]);
}
