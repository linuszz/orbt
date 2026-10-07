# Orbit

**A terminal workspace for local and remote machines, with built-in AI agent monitoring.**

Orbit (`orbt`) is a terminal multiplexer — like tmux — that also watches the AI coding agents running inside your sessions, lets you respond to them without leaving the terminal, and connects to remote machines over SSH with a single command. It adapts its layout automatically when the terminal is too narrow for a full desktop UI.

---

## Install

### One-liner (Linux / macOS)

```sh
curl -fsSL https://github.com/linuszz/orbt/releases/latest/download/install.sh | sh
```

### Homebrew (macOS / Linux)

```sh
brew install linuszz/orbt/orbt
```

### apt (Debian / Ubuntu)

```sh
curl -fsSL https://apt.orbt.sh/orbt.gpg.pub \
  | sudo gpg --dearmor -o /usr/share/keyrings/orbt.gpg
echo "deb [arch=amd64 signed-by=/usr/share/keyrings/orbt.gpg] https://apt.orbt.sh stable main" \
  | sudo tee /etc/apt/sources.list.d/orbt.list
sudo apt update && sudo apt install orbt
```

### AUR (Arch Linux)

```sh
yay -S orbt-bin     # prebuilt binary — fast
yay -S orbt         # build from source
```

### Scoop (Windows)

```powershell
scoop bucket add orbt https://github.com/linuszz/scoop-orbt
scoop install orbt
```

### winget (Windows)

```powershell
winget install Linus.Orbt
```

### Nix

```sh
# Run without installing
nix run github:linuszz/orbt

# Add to profile
nix profile install github:linuszz/orbt
```

### cargo

```sh
cargo install orbt
```

### Manual download

Prebuilt binaries for every platform are attached to each [GitHub Release](https://github.com/linuszz/orbt/releases/latest):

| Platform | File |
|---|---|
| Linux x86\_64 | `orbt-linux-x86_64.tar.gz` |
| macOS Apple Silicon | `orbt-macos-aarch64.tar.gz` |
| macOS Intel | `orbt-macos-x86_64.tar.gz` |
| Windows x86\_64 | `orbt-windows-x86_64.zip` |

SHA-256 sidecar files (`.sha256`) are included for each archive.

---

## Quick start

```sh
orbt           # start (auto-starts the background daemon on first run)
orbt daemon    # run the daemon in the foreground (for servers / init systems)
```

Orbit is client-server: `orbt` spawns a background daemon (`orbt daemon`) the first time it runs. The daemon keeps all sessions alive — close the client window without losing anything.

```sh
orbt kill      # stop the daemon and wipe its saved session
orbt --fresh   # start with a clean session, ignoring any saved state
```

---

## Remote connection

Connect Orbit's TUI directly to an `orbtd` daemon running on another machine:

```sh
orbt --remote user@host
orbt --remote user@host:2222     # custom port
```

Orbit opens a native SSH connection (reads your `~/.ssh/id_*` keys and `SSH_AUTH_SOCK`), forwards the remote Unix socket through the tunnel, and attaches as if the daemon were local. The remote daemon must be running (`orbt daemon` or a service unit).

---

## Layouts

Orbit ships two pane-arrangement strategies, switchable at runtime from **Settings → Layout** (`Ctrl+B ,`).

### Scrollable Strip (default)

Panes live in a horizontal band of columns that scrolls left and right — the same model as [niri](https://github.com/YaLTeR/niri). Each column can hold a single pane or a vertical stack of panes. The active column is always fully visible; the rest peek in from the sides.

```
 ◂  ╭─ 1/3 · editor ─────────────────╮╭─ 2/3 · server ──────────────────╮  ▸
    │ fn main() {                     ││ [2026-10-07 09:00] GET /api/data │
    │   println!("hello");            ││ 200 OK  12ms                     │
    │ }                               ││                                  │
    ╰─────────────────────────────────╯╰──────────────────────────────────╯
    ━━━━━━━━━━━━━━━◆━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ▦   +
```

**Navigating the strip**

| Action | How |
|---|---|
| Focus next / previous pane | `←` / `→` in command palette, or click the **◀ ▶** nav arrows |
| Scroll viewport without moving focus | Click the **◂ ▸** edge tabs on the frame border |
| Jump viewport to any position | Click on the nav rail (the scrollbar track) |
| Open pane overview | Click **▦** or press `Ctrl+B o` |
| Add a new column | Click **+** or `Ctrl+B c` |

**Resizing columns**

Each column carries its own width as a fraction of the band. Three ways to resize:

- Drag the vertical border between columns with the mouse.
- Click the **●** resize button on the pane's title bar to cycle through ¼ · ½ · ¾ of the band.
- Use `Ctrl+B }` / `Ctrl+B {` to nudge the active column wider or narrower.

**Stacking panes**

Drag a pane's title bar onto the middle third of another column to stack them vertically inside that column. Drag it out again (to the left or right third of any column) to give it its own column. `Ctrl+B "` splits the current column vertically.

**Edge tabs**

When columns extend off screen, a **◂** or **▸** notch appears on the left or right frame edge. Clicking it scrolls the viewport by the minimum distance to reveal the next hidden column — the same distance as pressing the nav arrow, so the animation feels identical.

**Viewport behaviour**

- Clicking a pane that is already fully visible never moves the viewport.
- Clicking a pane that is partially off screen glides the viewport to bring it fully into view, with the same eased animation as arrow-key navigation.
- The viewport is always stable during column resizes — the dragged edge follows the cursor 1:1 with live feedback.

### Split Tree (BSP)

Classic tmux-style binary splits. `Ctrl+B %` splits horizontally, `Ctrl+B "` splits vertically. Use `Ctrl+B z` to zoom a pane to fullscreen and back.

---

## Mobile / narrow terminal

When the terminal is narrower than 80 columns or shorter than 25 rows, Orbit automatically switches to a compact layout designed for phones, iPads, and small terminal windows.

The compact layout has four tabs across the bottom:

| Tab | What it shows |
|---|---|
| **TERMINAL** | Full-screen PTY — the active pane, with the strip nav bar |
| **SPACES** | Two-column switcher: workspaces (left) and tabs (right); tap a tab to go straight to it |
| **COMMAND** | Command palette — search and run any action |
| **AGENTS** | Agent list with status and scroll |

**Touch interactions on mobile**

- Tap the **◀ ▶** arrows or **▦** overview button in the nav bar to navigate.
- Tap the **◂ ▸** edge tabs on the frame border to peek at hidden panes.
- **Drag a pane's title bar** left or right to reorder it, or drop it on the centre of another pane to stack them. The drop target highlights in real time as you drag.
- All keyboard shortcuts that work on desktop also work in compact mode.

The layout switches back automatically when the window is resized above the threshold.

---

## Animation system

Orbit uses a tick-based easing engine (cubic ease-in/out, 16 ms ticks) for all motion. Animations can be toggled from **Settings → Animations** (`Ctrl+B ,`). Turning them off snaps everything instantly with no visual artifacts.

| Animation | What it does |
|---|---|
| **Strip scroll glide** | Viewport scrolls smoothly to bring the focused pane into view |
| **Click-focus glide** | Clicking a partially-visible pane glides the viewport to it |
| **Pane open bloom** | A new pane grows from its centre into its slot |
| **Overview open** | The pane overview card grid scales in when opened |
| **Agent pulse** | Working / blocked / errored agents pulse at different rates and colours |

---

## Features

### Session management

- Daemon-backed sessions that survive client disconnects
- Session restore on daemon startup — reopen exactly where you left off
- Multiple independent workspaces (spaces), each with its own tabs and panes
- Two pane layouts: **Scrollable Strip** (default) and **Split Tree** (BSP)
- Mouse-drag pane resizing, reordering, and stacking
- Scrollback with keyboard navigation
- Detach (`Ctrl+B d`) and reattach — the daemon keeps running
- `orbt kill` to wipe state; `orbt --fresh` to start clean

### Strip layout interaction

- Horizontal scrollable band — columns scroll off screen and peek back with edge tabs
- Per-column widths as band fractions, with three resize methods
- Vertical pane stacking within a column (one tab per stacked pane on the frame edge)
- Drag-to-reorder and drag-to-stack with live drop-target highlighting
- Pane overview (`▦`) with one card per pane, labelled by its working directory
- Stable viewport: click on a fully-visible pane never moves the strip

### AI agent monitoring

Orbit watches the processes running inside your panes and tracks AI coding agents (Claude Code, Codex, Aider, OpenCode, and others) automatically.

- Status tracking: **Transmitting** (working), **Eclipse** (blocked — needs you), **Standby** (idle), **Debris** (errored), done
- Live CPU / memory metrics and progress extraction from agent output
- **Eclipse modal**: when an agent is blocked, a floating panel lets you send a response without switching away from your work
- Agent Fleet panel: a sidebar listing all active agents with their status and duration
- Agent Detail modal: per-agent overlay with ACP tool calls, files touched, and token usage
- Status bar pulse: slow for working agents, fast amber for blocked

### Interface

- Command palette with type-to-filter search (`Ctrl+B` to open)
- Two built-in themes: **Orbit** (dark, purple accent) and **Orange** — switch at runtime with `Ctrl+B T`
- Traffic-light window controls on every pane border (close / zoom / resize)
- Rounded pane frames, modal dimming, open-bloom animation for new panes
- Right-click context menus on panes, tabs, and the sidebar
- Responsive layout: desktop strip or BSP on wide terminals, compact four-tab layout on narrow ones
- Settings panel (`Ctrl+B ,`) persisted to `~/.config/orbt/settings.toml`
- Status bar shows local time and UTC side by side; UTC is hidden automatically when space is tight

---

## Key bindings

All actions are also accessible through the command palette (`Ctrl+B`).

**Pane**

| Keys | Action |
|---|---|
| `Ctrl+B %` | Split pane horizontal (new column in strip) |
| `Ctrl+B "` | Split pane vertical (stack in current column) |
| `Ctrl+B x` | Close active pane |
| `Ctrl+B o` | Open pane overview |
| `Ctrl+B z` | Zoom pane (toggle fullscreen) |
| `Ctrl+B [` | Scrollback / copy mode — `j`/`k` scroll, `Esc` exit |
| `Ctrl+B I` | Paste image from clipboard into pane |
| `Ctrl+B }` | Widen active column |
| `Ctrl+B {` | Narrow active column |

**Strip navigation** (strip layout only)

| Keys | Action |
|---|---|
| `←` / `→` | Focus previous / next pane |
| Click **◀ ▶** | Focus previous / next pane (nav bar) |
| Click **◂ ▸** | Peek viewport left / right (edge tabs on frame) |
| Click **▦** | Open pane overview |
| Click **+** | Add new column |
| Drag title bar | Reorder column (drop left/right third) or stack (drop centre) |

**Tab**

| Keys | Action |
|---|---|
| `Ctrl+B c` | New tab |
| `Ctrl+B n` / `p` | Next / previous tab |

**View**

| Keys | Action |
|---|---|
| `Ctrl+B b` | Toggle workspace sidebar |
| `Ctrl+B a` | Toggle agent panel |
| `Ctrl+B T` | Cycle theme |
| `Ctrl+B ,` | Settings |
| `Ctrl+B ?` | Help |

**Session**

| Keys | Action |
|---|---|
| `Ctrl+B d` | Detach (Go Dark — session persists) |

**Agent panel** (after `Ctrl+B a`)

| Keys | Action |
|---|---|
| `j` / `k` | Navigate agent cards |
| `r` | Restart errored agent |
| `s` | Stop agent |
| `d` | Dismiss completed agent |
| `Esc` / `q` | Exit agent panel |

Mouse: click to focus, drag to resize panes, drag to reorder/stack (strip), right-click for context menu.

---

## Architecture

```
orbt --remote user@host         orbt (local)
         │                           │
         │ SSH tunnel (russh)         │ Unix socket
         ▼                           ▼
   remote orbtd ◄────────────► local orbtd
   (daemon)                    (daemon)
```

The `orbt` binary contains both the TUI client and the daemon (as `orbt daemon`). The daemon owns all PTYs and persists independently of the client.

```
crates/
├── orbt/            # TUI client + embedded daemon
├── orbt-protocol/   # IPC wire types (no tokio, publishable lib)
└── orbt-core/       # VT emulation and cell grid (no I/O, publishable lib)
```

Communication uses length-prefixed bincode over a Unix domain socket. Both client and daemon maintain independent VT parsers — same tradeoff as tmux.

---

## Building from source

Requires Rust 1.75+ and Linux or macOS (Windows is a secondary target).

```sh
git clone https://github.com/linuszz/orbt.git
cd orbt
cargo build -p orbt --release
```

With `just` installed:

```sh
just dev      # run the TUI client
just daemon   # run the daemon
just qa       # fmt-check + clippy + tests
```

System dependencies (Linux): `pkg-config libssl-dev cmake`

---

## Roadmap

| Status | Phase | Scope |
|---|---|---|
| Done | Mercury | Terminal workspace: panes, tabs, PTY, IPC, detach/reattach |
| Done | Mercury | Remote SSH connection (`orbt --remote`) |
| Done | Venus | Agent detection, monitoring, Eclipse intervention modal |
| Done | Venus | Compact / mobile TUI layout |
| Done | Venus | Scrollable strip layout with full interaction model |
| Done | Venus | Animation engine, rounded chrome, window controls |
| Planned | Earth | Clipboard sync over SSH (OSC 52 bridge) |
| Planned | Earth | Multi-protocol image rendering (Kitty / iTerm / Sixel) |
| Planned | Earth | File transfer channel |
| Planned | Mars | WASM plugin system and MCP client integration |

---

## License

Orbit is dual-licensed:

- **AGPL-3.0** for open-source and community use. See [LICENSE](LICENSE).
- **Commercial license** available on request for organizations that need to embed Orbit in a closed-source product or offer it as a managed service without disclosing modifications. Contact the maintainer.

Contributions require signing the [CLA](CLA.md).
