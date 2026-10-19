# herdr-lane

A [herdr](https://herdr.dev) plugin for [lane](https://lane.lukeed.com), lukeed's copy-on-write git worktrees.

Each lane opens as a linked worktree workspace, nested under its repo in the herdr sidebar. One key opens a popup menu that lists the repo's lanes and runs lane's own lifecycle commands on them.

## Why

herdr can already remove a worktree, but it bypasses lane's guard. `lane rm` keeps a lane that has pending notes, while herdr's worktree remove deletes the folder anyway. This plugin runs lane first and closes the herdr workspace only after lane succeeds, so lane decides what gets thrown away.

## Install

```sh
herdr plugin install luizbafilho/herdr-lane
```

Install builds the menu with `cargo build --release --locked`, so `cargo` must be on your `PATH`. `herdr plugin link` skips build commands, so after linking a checkout, run `cargo build --release` in it yourself.

Bind the menu in `~/.config/herdr/config.toml`:

```toml
[[keys.command]]
key = "prefix+t"
type = "plugin_action"
command = "lane.menu"
description = "lane menu"
```

Then run `herdr server reload-config`.

## Menu

The popup lists every lane with its branch, state, dirty flag and pending notes. `>` marks the lane you opened the menu from, and `*` marks a lane that is open in herdr. A panel beside the list shows the selected lane's path, workspace and last five commits. In a narrow popup the panel gives its room to the list.

Move with `j` and `k`, the arrow keys or the mouse. Commands act on the selected lane, which starts on the lane you came from. The key bar dims a command the selected lane cannot take.

| Key | Does |
| --- | --- |
| `n` | asks for a name and an optional base ref, runs `lane new`, then opens and focuses the workspace |
| `o` | opens the selected lane when it has no workspace yet, such as one an agent made with `lane new` |
| `m` | `lane merge`, then closes the lane's workspace and focuses the repo's |
| `p` | `lane push`, the workspace stays open |
| `r` | `lane rm`; if lane refuses, asks before `lane rm --force` |
| `x` | `lane prune`, then closes workspaces whose worktree is gone |
| `q`, `Esc` | quit; `Ctrl+C` quits from anywhere |

`n` and `o` close the popup once the new workspace has focus. The other commands keep it open, refresh the list and confirm with a toast. A spinner shows while a command runs. When one fails, its output opens in a dialog that scrolls with `j` and `k`, and `Enter` or `Esc` closes it.

The same handlers run without the popup, which is handy in scripts. Pass a command or its key, then a lane name. Without a name, the command acts on the lane in `LANE_CHECKOUT`. `new` takes a name and an optional base ref.

```sh
LANE_REPO_ROOT=/path/to/repo target/release/lane-menu merge fix-login
```

Without the popup, `rm` fails when lane refuses and never prompts.

## Native worktree creation

In a repo that ran `lane init`, herdr's own worktree creation makes a lane. That covers the sidebar's new worktree dialog, `prefix+shift+g` and `herdr worktree create`. The plugin listens for `worktree.created`, runs `lane new` on the branch herdr just made, opens the lane under the same label and removes herdr's checkout from `~/.herdr/worktrees`. The branch is kept as is.

Repos without `.lane` keep herdr's native worktrees. The plugin also leaves herdr's checkout in place when it is detached or already has changes. If `lane new` fails, the checkout goes back to its branch and a notification shows lane's error.

herdr's Remove worktree still bypasses lane, so remove lanes from the lane menu.

## Requirements

- `lane` and `jq` on the herdr server's `PATH`.
- A Rust toolchain with `cargo`, which builds the menu at install.
- A filesystem with reflinks (btrfs, XFS, APFS) for lane's warm cache of ignored files.
- herdr 0.9.0 or later, on Linux or macOS.

## License

MIT
