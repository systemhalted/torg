---
name: verify
description: Drive the torg TUI end-to-end in an isolated tmux session to verify editor changes at the real terminal surface.
---

# Verifying torg (torg-tui)

The surface is a full-screen TUI; unit tests cover the state tier, so verification means
driving the real binary and capturing panes.

## Build & launch

```sh
cargo build                                   # binary at target/debug/torg
DIR=$(mktemp -d) && printf '* A\nbody\n' > "$DIR/a.org"   # sample org files
tmux -L torgvfy new-session -d -s main -x 100 -y 24 \
  "cd $DIR && /abs/path/to/target/debug/torg a.org b.org; echo EXITED:\$?"
```

Always use a private socket (`-L torgvfy`) so the user's tmux is untouched; kill it with
`tmux -L torgvfy kill-server` when done.

## Drive & capture

```sh
tmux -L torgvfy send-keys -t main M-n        # Alt+N (buffer cycle); C-b, C-o, C-w, C-q, Tab…
tmux -L torgvfy send-keys -t main "text" Enter
tmux -L torgvfy capture-pane -p -t main      # the evidence; status line is the last row
```

Sleep ~0.3-0.5s after each send before capturing. The pane's `EXITED:$?` line only shows if
another pane keeps the session alive — the session dies with the shell, so capture *before*
the final quit or treat "server gone after Ctrl+Q" as a successful exit.

## Flows worth driving

- open two files from the CLI → status shows `[1/2] name`
- Alt+N/Alt+P cycle (fold with Tab in one buffer, switch away and back — folds/cursor persist)
- Ctrl+B picker (arrows/digits/Enter/Esc), Ctrl+O prompt (existing, missing → first save
  lands on the typed path, already-open → switches)
- Ctrl+W on dirty buffer → y/n prompt; on last buffer → fresh `[No Name]`, no quit
- Ctrl+Q with a dirty buffer → y/n guard

## Gotchas

- Keys arrive via crossterm: tmux names `M-n`, `C-q`, `Escape`, `Tab` all work.
- **Type text with `send-keys -l "..."`** — without `-l`, strings that happen to be key
  names match case-insensitively (`"end"` sends the End key, not e-n-d) and the "typed"
  text silently never arrives.
- Check cursor placement with `tmux display-message -p '#{cursor_x},#{cursor_y}'`.
- The status row is `capture-pane | tail -1` (reversed style doesn't survive capture; text does).
- `cargo run` inside tmux adds compile noise; run the built `target/debug/torg` directly.
