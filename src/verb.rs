use crate::lane::{Ctx, Failure, Lane, run};

/// What a handler printed on success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub output: String,
}

pub type Handled = Result<Outcome, Failure>;

/// What a verb acts on. Each variant carries the handler typed for its input,
/// so a verb cannot be dispatched with the wrong one.
#[derive(Debug, Clone, Copy)]
pub enum Target {
    /// The repo as a whole.
    None(fn(&Ctx) -> Handled),
    /// A lane that does not exist yet: a name and an optional base ref.
    NewLane(fn(&Ctx, &str, Option<&str>) -> Handled),
    AnyLane(fn(&Ctx, &Lane) -> Handled),
    /// A lane with no herdr workspace.
    ClosedLane(fn(&Ctx, &Lane) -> Handled),
}

impl Target {
    /// Whether the verb can run with `selected` as the chosen lane.
    pub fn accepts(self, selected: Option<&Lane>) -> bool {
        match self {
            Self::None(_) | Self::NewLane(_) => true,
            Self::AnyLane(_) => selected.is_some(),
            Self::ClosedLane(_) => selected.is_some_and(|lane| lane.workspace.is_none()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterSuccess {
    /// The verb moved focus to a workspace, so the popup is done.
    ExitPopup,
    /// Stay in the menu and confirm with a toast that reads `done`.
    Stay { done: &'static str },
}

#[derive(Debug)]
pub struct Verb {
    pub key: char,
    pub name: &'static str,
    pub label: &'static str,
    pub target: Target,
    pub after: AfterSuccess,
}

pub static VERBS: [Verb; 6] = [
    Verb {
        key: 'n',
        name: "new",
        label: "new",
        target: Target::NewLane(new_lane),
        after: AfterSuccess::ExitPopup,
    },
    Verb {
        key: 'o',
        name: "open",
        label: "open",
        target: Target::ClosedLane(open),
        after: AfterSuccess::ExitPopup,
    },
    Verb {
        key: 'm',
        name: "merge",
        label: "merge",
        target: Target::AnyLane(merge),
        after: AfterSuccess::Stay { done: "merged" },
    },
    Verb {
        key: 'p',
        name: "push",
        label: "push",
        target: Target::AnyLane(push),
        after: AfterSuccess::Stay { done: "pushed" },
    },
    Verb {
        key: 'r',
        name: "rm",
        label: "remove",
        target: Target::AnyLane(rm),
        after: AfterSuccess::Stay { done: "removed" },
    },
    Verb {
        key: 'x',
        name: "prune",
        label: "prune",
        target: Target::None(prune),
        after: AfterSuccess::Stay {
            done: "pruned landed lanes",
        },
    },
];

/// The verb named by `word`, which is either its key or its name.
pub fn find(word: &str) -> Option<&'static Verb> {
    VERBS
        .iter()
        .find(|verb| word == verb.name || word.chars().eq([verb.key]))
}

pub fn by_key(key: char) -> Option<&'static Verb> {
    VERBS.iter().find(|verb| verb.key == key)
}

fn new_lane(ctx: &Ctx, name: &str, base: Option<&str>) -> Handled {
    let mut command = ctx.lane(["new", name]);
    if let Some(base) = base {
        command.args(["--base", base]);
    }
    let output = command.stdin(std::process::Stdio::null()).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let printed = String::from_utf8_lossy(&output.stderr).into_owned() + &stdout;
    if !output.status.success() {
        return Err(Failure::Error(printed));
    }
    // `lane new` prints the new checkout's path on stdout and its progress on stderr.
    ctx.open_workspace(stdout.trim().as_ref(), name)?;
    Ok(Outcome { output: printed })
}

fn open(ctx: &Ctx, lane: &Lane) -> Handled {
    ctx.open_workspace(&lane.path, &lane.name)?;
    Ok(Outcome {
        output: String::new(),
    })
}

fn merge(ctx: &Ctx, lane: &Lane) -> Handled {
    let output = run(ctx.lane(["merge", &lane.name]))?;
    ctx.close_workspace(lane.workspace.as_deref(), &lane.path)?;
    Ok(Outcome { output })
}

fn push(ctx: &Ctx, lane: &Lane) -> Handled {
    let output = run(ctx.lane(["push", &lane.name]))?;
    Ok(Outcome { output })
}

/// `lane rm` keeps a lane with uncommitted work or pending notes, which
/// surfaces as [`Failure::Refused`] so the caller can offer [`rm_force`].
fn rm(ctx: &Ctx, lane: &Lane) -> Handled {
    let output = run(ctx.lane(["rm", &lane.name]))
        .map_err(|failure| Failure::Refused(failure.output().to_owned()))?;
    ctx.close_workspace(lane.workspace.as_deref(), &lane.path)?;
    Ok(Outcome { output })
}

pub fn rm_force(ctx: &Ctx, lane: &Lane) -> Handled {
    let output = run(ctx.lane(["rm", &lane.name, "--force"]))?;
    ctx.close_workspace(lane.workspace.as_deref(), &lane.path)?;
    Ok(Outcome { output })
}

/// `lane prune` does not say which lanes it removed, so a workspace is stale
/// once its checkout directory is gone.
fn prune(ctx: &Ctx) -> Handled {
    let output = run(ctx.lane(["prune"]))?;
    let mut stale = ctx.stale_workspaces()?;
    // Closing the popup's workspace kills the popup, so it goes last.
    stale.sort_by_key(|(_, checkout)| ctx.hosts_popup(checkout));
    for (id, checkout) in stale {
        ctx.close_workspace(Some(&id), &checkout)?;
    }
    Ok(Outcome { output })
}
