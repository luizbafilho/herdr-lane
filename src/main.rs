use std::{env, process::ExitCode};

use lane_menu::{cli, lane::Ctx, tui};

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
        None => tui::run(ctx),
        Some((word, rest)) => cli::run(&ctx, word, rest),
    }
}
