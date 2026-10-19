# herdr-lane

A [herdr](https://herdr.dev) plugin for [lane](https://lane.lukeed.com), lukeed's copy-on-write git worktrees.

Each lane opens as a linked worktree workspace, nested under its repo in the herdr sidebar. One key opens a popup menu that lists the repo's lanes and runs lane's own lifecycle commands on them.

## Why

herdr can already remove a worktree, but it bypasses lane's guard. `lane rm` keeps a lane that has pending notes, while herdr's worktree remove deletes the folder anyway. This plugin runs lane first and closes the herdr workspace only after lane succeeds, so lane decides what gets thrown away.

## Install

```sh
herdr plugin install luizbafilho/herdr-lane
```

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

The popup lists every lane with its branch, state, dirty flag and pending notes. `>` marks the lane you invoked the menu from, and `*` marks a lane that is open in herdr. A command acts on the current lane, or asks for a lane number when you started from the primary checkout.

| Key | Does |
| --- | --- |
| `n` | `lane new <name> [--base REF]`, then opens and focuses the workspace |
| `o` | opens a lane that has no workspace yet, such as one an agent made with `lane new` |
| `m` | `lane merge`, then closes the lane's workspace and focuses the repo's |
| `p` | `lane push`, the workspace stays open |
| `r` | `lane rm`; if lane refuses, asks before `lane rm --force` |
| `x` | `lane prune`, then closes workspaces whose worktree is gone |
| `q`, `Esc` | quit |

When a lane command fails, its output stays on screen until you press Enter.

The same handlers run without the popup, which is handy in scripts:

```sh
LANE_REPO_ROOT=/path/to/repo bin/lane-menu merge fix-login
```

## Native worktree creation

In a repo that ran `lane init`, herdr's own worktree creation makes a lane. That covers the sidebar's new worktree dialog, `prefix+shift+g` and `herdr worktree create`. The plugin listens for `worktree.created`, runs `lane new` on the branch herdr just made, opens the lane under the same label and removes herdr's checkout from `~/.herdr/worktrees`. The branch is kept as is.

Repos without `.lane` keep herdr's native worktrees. The plugin also leaves herdr's checkout in place when it is detached or already has changes. If `lane new` fails, the checkout goes back to its branch and a notification shows lane's error.

herdr's Remove worktree still bypasses lane, so remove lanes from the lane menu.

## Requirements

- `lane` and `jq` on the herdr server's `PATH`.
- A filesystem with reflinks (btrfs, XFS, APFS) for lane's warm cache of ignored files.
- herdr 0.9.0 or later, on Linux or macOS.

## License

MIT
