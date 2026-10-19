use std::{env, process::ExitCode};

use lane_menu::{cli, lane::Ctx};

fn main() -> ExitCode {
    let ctx = match Ctx::from_env() {
        Ok(ctx) => ctx,
        Err(message) => {
            eprintln!("lane-menu: {message}");
            return ExitCode::from(2);
        }
    };
    let args: Vec<String> = env::args().skip(1).collect();
    match args.split_first() {
        None => {
            eprintln!("usage: lane-menu <verb-or-key> [lane]");
            ExitCode::from(2)
        }
        Some((word, rest)) => cli::run(&ctx, word, rest),
    }
}
