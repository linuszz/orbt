# PROJECT KNOWLEDGE BASE

**Generated:** 2026-07-28
**Commit:** 7ed567b (v0.1.11)
**Branch:** main

Public release: **v0.1.11** (2026-07-28). Development HEAD is at this release.

## OVERVIEW

Terminal workspace multiplexer (tmux heritage) + first-class agent runtime. Rust workspace: 4 crates (`orbt` unified binary, `orbt-protocol` IPC, `orbt-core` VT emulation, `orbit-alias` crates.io placeholder). Phase 1 (Mercury) + Phase 2 (Venus agent runtime) in progress.

## STRUCTURE

```
orbit/
├── Cargo.toml / justfile
├── crates/
│   ├── orbt/           # Unified TUI client + embedded daemon (orbt binary)
│   │   └── src/
│   │       ├── app.rs / events.rs / ipc.rs / lib.rs / main.rs / ssh.rs
│   │       ├── daemon/
│   │       │   ├── mod.rs / agent.rs / io.rs / ipc.rs / pty.rs / session.rs
│   │       └── tui/
│   │           ├── mod.rs / theme.rs
│   │           └── widgets/           # 14 widget files
│   │               ├── agent_monitor.rs     # Satellites panel
│   │               ├── eclipse_modal.rs     # Blocked-agent intervention
│   │               ├── launch_modal.rs      # Agent type picker
│   │               ├── agent_detail_modal.rs     # ACP agent detail overlay
│   │               ├── spaces_sidebar.rs    # Multi-space sidebar
│   │               ├── command_palette.rs   # Flight Deck overlay
│   │               ├── status_bar.rs
│   │               ├── tab_bar.rs
│   │               ├── context_menu.rs
│   │               ├── settings_modal.rs
│   │               ├── mobile_nav.rs
│   │               ├── mobile_spaces.rs
│   │               └── mobile_confirm.rs
│   ├── orbt-protocol/ # IPC wire types -- NO tokio
│   ├── orbt-core/     # VT/CellGrid -- NO tokio
│   └── orbit-alias/   # crates.io name reservation (orbit v0.0.1 placeholder)
└── 02_design/          # Design specs (READ-ONLY)
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| IPC types/contract | `orbt-protocol/src/messages.rs` | Wire contract -- source of truth (41 variants: 25 ClientMessage + 16 ServerEvent) |
| Protocol encode/decode | `orbt-protocol/src/encoding.rs` | bincode 2.x serde helpers |
| TUI state + events | `orbt/src/app.rs` / `events.rs` | `App` struct: tabs, spaces, agents, modals; `events.rs` is 3274 lines |
| Server session/PTY | `orbt/src/daemon/session.rs` / `pty.rs` | Tab management, PTY spawn |
| **Agent runtime** | `orbt/src/daemon/agent.rs` | Detection, Eclipse, metrics -- `AgentRegistry` |
| Client IPC writer | `orbt/src/ipc.rs` | Background channel -- socket task |
| VT emulation | `orbt-core/src/vt/` | Cell grid, escape sequences |
| **Satellites panel UI** | `orbt/src/tui/widgets/agent_monitor.rs` | Card rendering, pulse animations |
| **Eclipse modal UI** | `orbt/src/tui/widgets/eclipse_modal.rs` | Blocked-agent intervention overlay |
| **Multi-space sidebar** | `orbt/src/tui/widgets/spaces_sidebar.rs` | Space cards, fleet badge |

## CODE MAP

| Symbol | Type | Location | Role |
|--------|------|----------|------|
| `ClientMessage` | enum | `orbt-protocol/src/messages.rs` | IPC request contract (25 variants, PROTOCOL_VERSION = 4) |
| `ServerEvent` | enum | `orbt-protocol/src/messages.rs` | IPC event contract (16 variants) |
| `Cell` | struct | `orbt-protocol/src/types.rs` | 16 bytes -- DO NOT grow |
| `App` | struct | `orbt/src/app.rs` | TUI state: tabs, spaces, agents, modals, selection, scroll |
| `SessionState` | struct | `orbt/src/daemon/session.rs` | Server session + tabs + agent wiring |
| `AgentRegistry` | struct | `orbt/src/daemon/agent.rs` | Agent detection, state machine, metrics |
| `AgentRegistry::watch_pane()` | fn | `orbt/src/daemon/agent.rs` | Per-pane async task: /proc scan + PTY scan |
| `EclipseModalState` | struct | `orbt/src/app.rs` | Blocked agent intervention modal state |
| `LaunchModalState` | struct | `orbt/src/app.rs` | Agent launcher picker state |

## CONVENTIONS

- **Prefix key**: `Ctrl+B` tmux-style, intercepted before PTY
- **Aerospace metaphor**: Space/Pane/Agent in code; Deck/Port/Satellite in brand
- **OKLch / RGB color tokens**: Default theme is `orbt` (Tokyo Night-inspired, purple accent); `orange` theme provides the classic orange-accent dark theme (`#d97706`)
- **3 button states**: Default/Hover/Active only
- **No emoji**: Unicode symbols only (`●○◎◉◌×≡▸`)
- **TOML only**: No YAML config
- **tokio-free libs**: `orbt-protocol` + `orbt-core` -- NO `tokio` dep
- **Agent panel modes**: Hidden / Sidebar / Modal -- cycled with `Ctrl+B a`
- **Agent panel keys**: `j`/`k` scroll, `r` restart Error agent, `s` stop, `d` dismiss, `Esc`/`q` exit
- **Animation**: Lerp-based pulse (Working=slow, Blocked=fast amber, Error=red blink)
- **Agent sort**: Blocked-first, then Working, then others; stable on updates
- **Agent Fleet default**: Hidden behind `agent_fleet_enabled` config flag (default off in v0.1.11+); enable in settings.toml or via Ctrl+B a

## ANTI-PATTERNS (THIS PROJECT)

- **NEVER** `as any` / `@ts-ignore` / `unwrap()` in library public APIs
- **NEVER** suppress clippy without justifying comment
- **NEVER** busy-loop render -- redraw on demand (`needs_redraw`)
- **NEVER** hold `RwLock.write()` across `await` that reads same lock (self-deadlock)
- **NEVER** add heap fields to `Cell` (16 byte invariant)
- **NEVER** touch PTY from client -- all server-side

## UNIQUE STYLES

- Aerospace metaphor branding: "Orbit/Ground Station/Core", "Port/Deck/Satellite"
- Command Palette via `Flight Deck` (prefix key opens)
- Server-side tabs: `TabId`, `new_tab`/`switch_tab`/`close_tab` survive reconnect
- **Satellite Eclipse**: PTY output pattern scanning detects blocked agents (40+ block patterns)
- **Dual agent detection**: `/proc` child scanning (500ms) + PTY output scanning (event-driven)
- **Agent metrics**: CPU% (tick delta), RSS (VmRSS), progress% (regex from output)
- **Multi-space**: Adjective-noun naming generator, per-space agent fleet
- **Unified binary**: `orbt` ships both TUI client and embedded daemon (no separate `orbitd` crate); the runtime log name `orbtd` may still appear internally
- **SSH connections**: `orbt --remote user@host[:port]` -- reads `~/.ssh/id_*` keys, `SSH_AUTH_SOCK`, validates known_hosts; supports key/password/passphrase auth
- **Mobile/narrow TUI**: Auto-switches to 4-tab compact layout below 80 columns or 25 rows (TERMINAL / SPACES / COMMAND / AGENTS)
- **Payload/image paste**: `Ctrl+B I` pastes clipboard images; `UploadPayload` IPC sends them to the remote daemon
- **OpenCode agent detection**: Detects OpenCode alongside Claude/Codex/Aider/GH-Copilot/Cursor and script runners
- **Agent Detail Modal**: Per-agent overlay showing ACP tool calls, files touched, token usage; opened via [Detail] button on agent cards

## COMMANDS

```bash
nix-shell -p gcc --run "cargo build --workspace"     # build (gcc for NixOS)
nix-shell -p gcc --run "cargo clippy --workspace --all-targets -- -D warnings"
nix-shell -p gcc --run "cargo test --workspace"
nix-shell -p gcc --run "cargo fmt --all --check"

# Canonical run commands (justfile recipes are stale -- they still target removed packages)
cargo run -p orbt                # run the TUI client
cargo run -p orbt -- daemon      # run the daemon in the foreground

just qa                          # fmt-check + clippy + test
```

## NOTES

- `orbt-protocol` + `orbt-core` are tokio-free for unit test isolation
- Client never touches PTY -- server owns all PTYs
- Both sides run VT parsers (accepted 2x CPU tradeoff)
- `Cell` must stay 16 bytes -- grid clone ~160KB
- Socket path: `$XDG_RUNTIME_DIR/orbt.sock` -- `$TMPDIR/orbt-<uid>.sock`
- Protocol: length-prefixed bincode (4MB max); current version is PROTOCOL_VERSION = 4 (bumped from 3 in v0.1.11 — breaking: added AgentAcpUpdated event + AcpDetail types)
- Async lock rule: scope write guards tight; release before any `await` that reads same state
- Agent detection: `AgentRegistry::watch_pane()` scans last 256 bytes of PTY output for block patterns
- Agent names matched: `claude`, `codex`, `aider`, `gh-copilot`, `cursor`, `opencode` (+ script runners `node`/`npx`/`python`)
- **ACP protocol detection**: agents exposing ACP (Agent Client Protocol) get [ACP] badge + detail modal with tool-call tracking
- PTY output is ANSI-stripped before display in agent fields (`strip_ansi`)
- `events.rs` is the largest file (3274 lines) -- all key/mouse dispatch lives here
- Settings persisted to `~/.config/orbt/settings.toml`

## RELEASE MANAGEMENT

All release planning, channel status, version history, and release todos are maintained in:

**`/home/linus/dev/00_orbit/03_release/RELEASE_STATUS.md`** -- single canonical source for release work.

Current public release is **v0.1.11** (2026-07-28). Channels: GitHub Releases, install.sh, crates.io (`orbt`/`orbt-protocol`/`orbt-core`), Homebrew tap, apt (apt.orbt.sh), AUR (`orbt-bin`/`orbt`/`orbit`), Scoop, winget (PR #404264 pending review), Nix flake. See RELEASE_STATUS.md for full channel details, pending issues, and version history.
