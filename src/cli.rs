//! `lane-menu <verb-or-key> [args]`: one handler, no popup, for scripts.

use std::process::ExitCode;

use crate::{
    lane::{Ctx, Failure, Lane},
    verb::{self, Handled, Target, Verb},
};

pub fn run(ctx: &Ctx, word: &str, args: &[String]) -> ExitCode {
    let Some(verb) = verb::find(word) else {
        eprintln!("lane-menu: unknown command: {word}");
        return ExitCode::from(2);
    };
    match dispatch(ctx, verb, args) {
        Ok(outcome) => {
            print!("{}", outcome.output);
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprint!("{}", failure.output());
            ExitCode::FAILURE
        }
    }
}

fn dispatch(ctx: &Ctx, verb: &Verb, args: &[String]) -> Handled {
    match verb.target {
        Target::None(handler) => handler(ctx),
        Target::NewLane(handler) => {
            let name = args
                .first()
                .ok_or_else(|| error("usage: lane-menu new <name> [base]"))?;
            handler(ctx, name, args.get(1).map(String::as_str))
        }
        Target::AnyLane(handler) | Target::ClosedLane(handler) => {
            let lane = pick(ctx, verb, args.first())?;
            handler(ctx, &lane)
        }
    }
}

/// The lane named on the command line, or else the one `LANE_CHECKOUT` is in.
fn pick(ctx: &Ctx, verb: &Verb, name: Option<&String>) -> Result<Lane, Failure> {
    let lanes = ctx.lanes()?;
    let lane = match name {
        Some(name) => lanes
            .into_iter()
            .find(|lane| &lane.name == name)
            .ok_or_else(|| error(&format!("no such lane: {name}")))?,
        None => lanes
            .into_iter()
            .find(|lane| ctx.is_current(lane))
            .ok_or_else(|| error(&format!("usage: lane-menu {} <lane>", verb.name)))?,
    };
    if !verb.target.accepts(Some(&lane)) {
        return Err(error(&format!("{} is already open in herdr", lane.name)));
    }
    Ok(lane)
}

fn error(message: &str) -> Failure {
    Failure::Error(format!("{message}\n"))
}
