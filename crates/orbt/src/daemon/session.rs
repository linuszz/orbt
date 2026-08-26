use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Read the current working directory of a process from the OS.
/// Falls back to `fallback` if the pid is unknown or the OS call fails.
fn proc_cwd(_pid: u32, fallback: &str) -> String {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let pid = _pid;
    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/{}/cwd", pid);
        if let Ok(p) = std::fs::read_link(&path) {
            if let Some(s) = p.to_str() {
                return s.to_string();
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        // proc_pidinfo with PROC_PIDVNODEPATHINFO is the right call but requires
        // a C binding. Use `lsof` as a portable fallback for now.
        let out = std::process::Command::new("lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output();
        if let Ok(o) = out {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                if let Some(p) = line.strip_prefix('n') {
                    return p.to_string();
                }
            }
        }
    }
    fallback.to_string()
}

use anyhow::Context;
use orbt_protocol::{
    CellGrid, FullState, PaneId, PaneInfo, PaneLayout, ServerEvent, SpaceId, SpaceInfo, SplitDir,
    TabId, TabInfo,
};
use portable_pty::PtySize;
use tokio::sync::{broadcast, mpsc, RwLock};

use super::agent::AgentRegistry;
use super::pty::{self, SharedChild, SharedMaster, SharedVtParser};

const ADJECTIVES: &[&str] = &[
    "cosmic", "stellar", "quantum", "lunar", "solar", "orbital", "deep", "silent", "swift", "apex",
    "delta", "zenith", "polar", "radiant", "binary", "axial", "thermal", "mach", "ion", "photon",
];

const NOUNS: &[&str] = &[
    "mars", "void", "nova", "horizon", "nebula", "atlas", "vega", "lyra", "cygnus", "orbt",
    "pulse", "core", "arc", "link", "beacon", "vector", "node", "flux", "rift", "zone",
];

pub fn generate_space_name(existing: &[&str]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    // Seed from current time nanos — good enough for name generation.
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(42);

    for attempt in 0..10 {
        let mut h = DefaultHasher::new();
        (seed + attempt).hash(&mut h);
        let v = h.finish() as usize;
        let adj = ADJECTIVES[v % ADJECTIVES.len()];
        let noun = NOUNS[(v / ADJECTIVES.len()) % NOUNS.len()];
        let candidate = format!("{adj}-{noun}");
        if !existing.contains(&candidate.as_str()) {
            return candidate;
        }
    }
    // Fallback: pick a fixed adj-noun pair and increment a counter until unique.
    let mut h = DefaultHasher::new();
    seed.hash(&mut h);
    let v = h.finish() as usize;
    let adj = ADJECTIVES[v % ADJECTIVES.len()];
    let noun = NOUNS[(v / ADJECTIVES.len()) % NOUNS.len()];
    let mut n = 2u32;
    loop {
        let candidate = format!("{adj}-{noun}-{n}");
        if !existing.contains(&candidate.as_str()) {
            return candidate;
        }
        n += 1;
    }
}

pub struct PaneEntry {
    pub input_tx: mpsc::Sender<Vec<u8>>,
    pub vt_parser: SharedVtParser,
    pub master: SharedMaster,
    pub child: SharedChild,
}

pub struct TabState {
    pub name: String,
    pub layout: PaneLayout,
    pub active_pane: PaneId,
}

pub struct SessionState {
    pub space_id: SpaceId,
    pub space_name: String,
    pub panes: RwLock<HashMap<PaneId, PaneEntry>>,
    pub tabs: RwLock<HashMap<TabId, TabState>>,
    pub tab_order: RwLock<Vec<TabId>>,
    pub active_tab: RwLock<TabId>,
    pub next_pane_id: Arc<AtomicU32>,
    pub next_tab_id: Arc<AtomicU32>,
    pub event_bus: broadcast::Sender<ServerEvent>,
    pub shell: String,
    pub cwd: String,
    pub agent_registry: Arc<AgentRegistry>,
    /// Circular scrollback buffer per pane: last 500 non-empty lines of stripped PTY output.
    /// Populated by `spawn_scrollback_collector`; consumed by `to_snapshot` at shutdown.
    pub pane_scrollback: Arc<RwLock<HashMap<PaneId, VecDeque<String>>>>,
}

impl SessionState {
    // Standalone constructor with self-owned counters; kept for test/standalone use.
    #[allow(dead_code)]
    pub async fn new(
        event_bus: broadcast::Sender<ServerEvent>,
        shell: String,
        cwd: String,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<Self> {
        let pane_id = PaneId(0);
        let space_id = SpaceId(0);
        let tab_id = TabId(0);
        let agent_registry = AgentRegistry::new(event_bus.clone());
        let handles = pty::spawn_pty(pane_id, &shell, &cwd, cols, rows, event_bus.clone()).await?;

        if let Some(pid) = handles.child_pid {
            Arc::clone(&agent_registry).watch_pane(pane_id, space_id, pid);
        }

        let mut panes = HashMap::new();
        panes.insert(
            pane_id,
            PaneEntry {
                input_tx: handles.input_tx,
                vt_parser: handles.parser,
                master: handles.master,
                child: handles.child,
            },
        );

        let mut tabs = HashMap::new();
        tabs.insert(
            tab_id,
            TabState {
                name: "dev".to_string(),
                layout: PaneLayout::Leaf(pane_id),
                active_pane: pane_id,
            },
        );

        Ok(Self {
            space_id,
            space_name: generate_space_name(&[]),
            panes: RwLock::new(panes),
            tabs: RwLock::new(tabs),
            tab_order: RwLock::new(vec![tab_id]),
            active_tab: RwLock::new(tab_id),
            next_pane_id: Arc::new(AtomicU32::new(1)),
            next_tab_id: Arc::new(AtomicU32::new(1)),
            event_bus,
            shell,
            cwd,
            agent_registry,
            pane_scrollback: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a session with shared pane/tab ID counters (used by SpaceManager).
    // All arguments are distinct required inputs; a builder would be heavier than necessary.
    #[allow(clippy::too_many_arguments)]
    pub async fn with_counters(
        event_bus: broadcast::Sender<ServerEvent>,
        shell: String,
        cwd: String,
        cols: u16,
        rows: u16,
        space_id: SpaceId,
        space_name: String,
        next_pane_id: Arc<AtomicU32>,
        next_tab_id: Arc<AtomicU32>,
        agent_registry: Arc<AgentRegistry>,
    ) -> anyhow::Result<Self> {
        let pane_id = PaneId(next_pane_id.fetch_add(1, Ordering::Relaxed));
        let tab_id = TabId(next_tab_id.fetch_add(1, Ordering::Relaxed));
        let tab_name = "tab0".to_string();

        let handles = pty::spawn_pty(pane_id, &shell, &cwd, cols, rows, event_bus.clone()).await?;

        if let Some(pid) = handles.child_pid {
            Arc::clone(&agent_registry).watch_pane(pane_id, space_id, pid);
        }

        let mut panes = HashMap::new();
        panes.insert(
            pane_id,
            PaneEntry {
                input_tx: handles.input_tx,
                vt_parser: handles.parser,
                master: handles.master,
                child: handles.child,
            },
        );

        let mut tabs = HashMap::new();
        tabs.insert(
            tab_id,
            TabState {
                name: tab_name,
                layout: PaneLayout::Leaf(pane_id),
                active_pane: pane_id,
            },
        );

        Ok(Self {
            space_id,
            space_name,
            panes: RwLock::new(panes),
            tabs: RwLock::new(tabs),
            tab_order: RwLock::new(vec![tab_id]),
            active_tab: RwLock::new(tab_id),
            next_pane_id,
            next_tab_id,
            event_bus,
            shell,
            cwd,
            agent_registry,
            pane_scrollback: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn split_pane(&self, tab_id: TabId, direction: SplitDir) -> anyhow::Result<PaneId> {
        let new_id = PaneId(self.next_pane_id.fetch_add(1, Ordering::Relaxed));
        let active = {
            let tabs = self.tabs.read().await;
            let tab = tabs
                .get(&tab_id)
                .ok_or_else(|| anyhow::anyhow!("tab not found"))?;
            tab.active_pane
        };
        let (cols, rows) = self.active_pane_size(&tab_id).await;

        let handles = pty::spawn_pty(
            new_id,
            &self.shell,
            &self.cwd,
            cols,
            rows,
            self.event_bus.clone(),
        )
        .await?;

        if let Some(pid) = handles.child_pid {
            Arc::clone(&self.agent_registry).watch_pane(new_id, self.space_id, pid);
        }

        {
            let mut panes = self.panes.write().await;
            panes.insert(
                new_id,
                PaneEntry {
                    input_tx: handles.input_tx,
                    vt_parser: handles.parser,
                    master: handles.master,
                    child: handles.child,
                },
            );
        }

        {
            let mut tabs = self.tabs.write().await;
            if let Some(tab) = tabs.get_mut(&tab_id) {
                tab.layout.split_leaf(active, direction, new_id);
                tab.active_pane = new_id;
            }
        }

        let _ = self
            .event_bus
            .send(ServerEvent::SpaceUpdated(self.collect_space_info().await));
        Ok(new_id)
    }

    pub async fn close_pane(&self, tab_id: TabId, pane_id: PaneId) {
        {
            let mut panes = self.panes.write().await;
            if let Some(entry) = panes.remove(&pane_id) {
                if let Ok(mut child) = entry.child.lock() {
                    let _ = child.kill();
                }
            }
        }

        let mut removed_tab = false;
        {
            let mut tabs = self.tabs.write().await;
            if let Some(tab) = tabs.get_mut(&tab_id) {
                tab.layout.remove_leaf(pane_id);
                let leaves = tab.layout.leaves();
                tab.active_pane = leaves.first().copied().unwrap_or(tab.active_pane);
                if leaves.is_empty() {
                    tabs.remove(&tab_id);
                    removed_tab = true;
                }
            }
        }

        if removed_tab {
            let mut order = self.tab_order.write().await;
            order.retain(|&id| id != tab_id);
            let mut active = self.active_tab.write().await;
            if *active == tab_id {
                *active = order.first().copied().unwrap_or(TabId(u32::MAX));
            }
        }

        let total_panes: usize = {
            let tabs = self.tabs.read().await;
            tabs.values().map(|t| t.layout.leaves().len()).sum()
        };
        if total_panes == 0 {
            let _ = self.event_bus.send(ServerEvent::SpaceClosed(self.space_id));
            return;
        }

        let _ = self
            .event_bus
            .send(ServerEvent::SpaceUpdated(self.collect_space_info().await));
    }

    pub async fn new_tab(&self, name: Option<String>) -> anyhow::Result<TabId> {
        let new_id = TabId(self.next_tab_id.fetch_add(1, Ordering::Relaxed));
        let tab_count = self.tab_order.read().await.len();
        let name = name.unwrap_or_else(|| format!("tab{}", tab_count));
        let pane_id = PaneId(self.next_pane_id.fetch_add(1, Ordering::Relaxed));

        let (cols, rows) = {
            let active_tab_id = *self.active_tab.read().await;
            self.active_pane_size(&active_tab_id).await
        };

        let handles = pty::spawn_pty(
            pane_id,
            &self.shell,
            &self.cwd,
            cols,
            rows,
            self.event_bus.clone(),
        )
        .await
        .context("failed to spawn PTY for new tab")?;

        if let Some(pid) = handles.child_pid {
            Arc::clone(&self.agent_registry).watch_pane(pane_id, self.space_id, pid);
        }

        {
            let mut panes = self.panes.write().await;
            panes.insert(
                pane_id,
                PaneEntry {
                    input_tx: handles.input_tx,
                    vt_parser: handles.parser,
                    master: handles.master,
                    child: handles.child,
                },
            );
        }

        {
            let mut tabs = self.tabs.write().await;
            tabs.insert(
                new_id,
                TabState {
                    name,
                    layout: PaneLayout::Leaf(pane_id),
                    active_pane: pane_id,
                },
            );
        }
        {
            let mut order = self.tab_order.write().await;
            order.push(new_id);
        }
        {
            *self.active_tab.write().await = new_id;
        }

        let _ = self
            .event_bus
            .send(ServerEvent::SpaceUpdated(self.collect_space_info().await));
        Ok(new_id)
    }

    pub async fn close_tab(&self, tab_id: TabId) {
        {
            let mut tabs = self.tabs.write().await;
            if let Some(tab) = tabs.remove(&tab_id) {
                let mut panes = self.panes.write().await;
                for leaf in tab.layout.leaves() {
                    if let Some(entry) = panes.remove(&leaf) {
                        if let Ok(mut child) = entry.child.lock() {
                            let _ = child.kill();
                        }
                    }
                }
            }
        }
        {
            let mut order = self.tab_order.write().await;
            order.retain(|&id| id != tab_id);
        }
        {
            let mut active = self.active_tab.write().await;
            if *active == tab_id {
                *active = self
                    .tab_order
                    .read()
                    .await
                    .first()
                    .copied()
                    .unwrap_or(TabId(u32::MAX));
            }
        }

        let total_panes: usize = {
            let tabs = self.tabs.read().await;
            tabs.values().map(|t| t.layout.leaves().len()).sum()
        };
        if total_panes == 0 {
            let _ = self.event_bus.send(ServerEvent::SpaceClosed(self.space_id));
            return;
        }

        let _ = self
            .event_bus
            .send(ServerEvent::SpaceUpdated(self.collect_space_info().await));
    }

    pub async fn switch_tab(&self, tab_id: TabId) {
        {
            let mut active = self.active_tab.write().await;
            *active = tab_id;
        }
        let _ = self
            .event_bus
            .send(ServerEvent::SpaceUpdated(self.collect_space_info().await));
    }

    pub async fn reorder_tab(&self, tab_id: TabId, to_index: usize) {
        {
            let mut order = self.tab_order.write().await;
            if let Some(from) = order.iter().position(|&id| id == tab_id) {
                let to = to_index.min(order.len().saturating_sub(1));
                if from != to {
                    order.remove(from);
                    order.insert(to, tab_id);
                }
            }
        }
        let _ = self
            .event_bus
            .send(ServerEvent::SpaceUpdated(self.collect_space_info().await));
    }

    pub async fn resize_split(
        &self,
        _tab_id: TabId,
        first_pane: PaneId,
        second_pane: PaneId,
        ratio: f32,
    ) {
        {
            let mut tabs = self.tabs.write().await;
            for tab in tabs.values_mut() {
                if tab.layout.set_split_ratio(first_pane, second_pane, ratio) {
                    break;
                }
            }
        }
        let _ = self
            .event_bus
            .send(ServerEvent::SpaceUpdated(self.collect_space_info().await));
    }

    pub async fn send_input(&self, _tab_id: TabId, pane_id: PaneId, data: Vec<u8>) {
        let panes = self.panes.read().await;
        if let Some(entry) = panes.get(&pane_id) {
            let _ = entry.input_tx.send(data).await;
        }
    }

    pub async fn resize_pane(&self, _tab_id: TabId, pane_id: PaneId, cols: u16, rows: u16) {
        let panes = self.panes.read().await;
        if let Some(entry) = panes.get(&pane_id) {
            if let Ok(master) = entry.master.lock() {
                let _ = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            if let Ok(mut parser) = entry.vt_parser.lock() {
                parser.grid.resize(cols, rows);
            }
        }
    }

    /// Send SIGWINCH to all PTY children by re-issuing resize at the current size.
    /// Called on client connect so that idle TUI apps (Claude Code, vim, yazi) redraw
    /// and emit fresh output — including correct cursor_visible state.
    pub async fn nudge_all_panes(&self) {
        let panes = self.panes.read().await;
        for entry in panes.values() {
            let (cols, rows) = {
                let parser = entry.vt_parser.lock().unwrap();
                (parser.grid.cols, parser.grid.rows)
            };
            if let Ok(master) = entry.master.lock() {
                let _ = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
        }
    }

    pub async fn focus_pane(&self, tab_id: TabId, pane_id: PaneId) {
        {
            let mut tabs = self.tabs.write().await;
            if let Some(tab) = tabs.get_mut(&tab_id) {
                tab.active_pane = pane_id;
            }
        }
        {
            *self.active_tab.write().await = tab_id;
        }
        let _ = self
            .event_bus
            .send(ServerEvent::SpaceUpdated(self.collect_space_info().await));
    }

    pub async fn active_pane_size(&self, tab_id: &TabId) -> (u16, u16) {
        let tabs = self.tabs.read().await;
        if let Some(tab) = tabs.get(tab_id) {
            if let Some(entry) = self.panes.read().await.get(&tab.active_pane) {
                if let Ok(g) = entry.vt_parser.lock() {
                    return (g.grid.cols, g.grid.rows);
                }
            }
        }
        (80, 24)
    }

    /// Spawn a background task that subscribes to the event bus and accumulates
    /// stripped PTY output into `pane_scrollback` (capped at 500 lines per pane).
    pub fn spawn_scrollback_collector(self: Arc<Self>) {
        let scrollback = Arc::clone(&self.pane_scrollback);
        let mut rx = self.event_bus.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ServerEvent::PaneOutput { pane_id, data }) => {
                        let text = std::str::from_utf8(&data).unwrap_or("");
                        let stripped = super::agent::strip_ansi(text);
                        let mut sb = scrollback.write().await;
                        let buf = sb.entry(pane_id).or_insert_with(VecDeque::new);
                        for line in stripped.lines() {
                            let trimmed = line.trim();
                            // Filter terminal protocol garbage that slips through strip_ansi:
                            // - DCS/XTGETTCAP hex payloads ("+q..." after stripping the \x1bP)
                            // - Isolated backspace characters (\x08 keystroke echoes)
                            // - Lines that are purely non-printable after trimming
                            if trimmed.is_empty()
                                || trimmed.starts_with("+q")
                                || trimmed.chars().all(|c| c.is_ascii_control())
                            {
                                continue;
                            }
                            buf.push_back(trimmed.to_string());
                            if buf.len() > 500 {
                                buf.pop_front();
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        });
    }

    /// Serialize this session into a `SpaceSnapshot` for on-disk persistence.
    pub async fn to_snapshot(&self) -> crate::daemon::snapshot::SpaceSnapshot {
        let tab_order = self.tab_order.read().await;
        let tabs = self.tabs.read().await;
        let panes = self.panes.read().await;
        let scrollback = self.pane_scrollback.read().await;
        let active_tab = *self.active_tab.read().await;

        let mut tab_snaps = Vec::new();
        for tab_id in tab_order.iter() {
            if let Some(tab) = tabs.get(tab_id) {
                let leaf_ids = tab.layout.leaves();
                let mut pane_snaps = Vec::new();
                for pane_id in &leaf_ids {
                    let cwd = if let Some(entry) = panes.get(pane_id) {
                        let child_pid = entry.child.lock().ok().and_then(|c| c.process_id());
                        child_pid
                            .map(|p| proc_cwd(p, &self.cwd))
                            .unwrap_or_else(|| self.cwd.clone())
                    } else {
                        self.cwd.clone()
                    };
                    let lines = scrollback
                        .get(pane_id)
                        .map(|d| d.iter().cloned().collect::<Vec<_>>())
                        .unwrap_or_default();
                    pane_snaps.push(crate::daemon::snapshot::PaneSnapshot {
                        id: pane_id.0,
                        cwd,
                        scrollback: lines,
                    });
                }
                tab_snaps.push(crate::daemon::snapshot::TabSnapshot {
                    id: tab_id.0,
                    name: tab.name.clone(),
                    active_pane_id: tab.active_pane.0,
                    panes: pane_snaps,
                    layout: tab.layout.clone(),
                });
            }
        }

        // Collect agents belonging to this space from the shared registry.
        let all_agents = self.agent_registry.get_agents().await;
        let agents: Vec<_> = all_agents
            .into_iter()
            .filter(|a| a.space_id == self.space_id)
            .map(|a| crate::daemon::snapshot::AgentSnapshot {
                name: a.name,
                launch_cmd: a.launch_cmd,
                cwd: self.cwd.clone(),
            })
            .collect();

        crate::daemon::snapshot::SpaceSnapshot {
            id: self.space_id.0,
            name: self.space_name.clone(),
            cwd: self.cwd.clone(),
            active_tab_id: active_tab.0,
            tabs: tab_snaps,
            agents,
        }
    }

    /// Reconstruct a `SessionState` from a saved `SpaceSnapshot`.
    /// Spawns one PTY shell per pane at the saved `cwd`, pre-populates the in-memory scrollback
    /// ring buffer so history survives into the next snapshot cycle, and schedules a 1-second
    /// delayed re-launch for any saved agent commands.
    pub async fn restore_from_snapshot(
        snap: &crate::daemon::snapshot::SpaceSnapshot,
        event_bus: broadcast::Sender<ServerEvent>,
        agent_registry: Arc<AgentRegistry>,
        next_pane_id: Arc<AtomicU32>,
        next_tab_id: Arc<AtomicU32>,
        shell: String,
    ) -> anyhow::Result<Self> {
        let space_id = SpaceId(snap.id);

        let mut panes_map: HashMap<PaneId, PaneEntry> = HashMap::new();
        let mut tabs_map: HashMap<TabId, TabState> = HashMap::new();
        let mut tab_order: Vec<TabId> = Vec::new();
        let pane_scrollback = Arc::new(RwLock::new(HashMap::<PaneId, VecDeque<String>>::new()));

        for tab_snap in &snap.tabs {
            let tab_id = TabId(tab_snap.id);
            let mut pane_ids_in_tab: Vec<PaneId> = Vec::new();

            for pane_snap in &tab_snap.panes {
                let pane_id = PaneId(pane_snap.id);
                pane_ids_in_tab.push(pane_id);

                let handles = match pty::spawn_pty(
                    pane_id,
                    &shell,
                    &pane_snap.cwd,
                    80,
                    24,
                    event_bus.clone(),
                )
                .await
                .with_context(|| format!("restore: failed to spawn PTY for pane {}", pane_id.0))
                {
                    Ok(h) => h,
                    Err(e) => {
                        // Kill every PTY that was already spawned in this restore attempt
                        // to avoid leaking child processes.
                        for entry in panes_map.values() {
                            if let Ok(mut child) = entry.child.lock() {
                                let _ = child.kill();
                            }
                        }
                        return Err(e);
                    }
                };

                if let Some(pid) = handles.child_pid {
                    Arc::clone(&agent_registry).watch_pane(pane_id, space_id, pid);
                }

                // Apply the same filter as the scrollback collector so old snapshots
                // that contain XTGETTCAP hex payloads or keystroke echoes are cleaned
                // up before being replayed.
                let clean_lines: Vec<&str> = pane_snap
                    .scrollback
                    .iter()
                    .map(|l| l.trim())
                    .filter(|l| {
                        !l.is_empty()
                            && !l.starts_with("+q")
                            && !l.chars().all(|c| c.is_ascii_control())
                    })
                    .collect();

                if !clean_lines.is_empty() {
                    // Build the history text and replay it through the server-side VT
                    // parser. These are all sync operations (no await), so the spawned
                    // PTY output task has not had a chance to run yet — no interleave risk.
                    let history_text = clean_lines.join("\r\n") + "\r\n";
                    if let Ok(mut parser) = handles.parser.lock() {
                        parser.process(history_text.as_bytes());
                    }
                    // Broadcast so any already-connected client can also update its
                    // local VT parser. (No subscribers at daemon startup → ignored.)
                    let _ = event_bus.send(ServerEvent::PaneOutput {
                        pane_id,
                        data: history_text.into_bytes(),
                    });

                    // Pre-populate the ring buffer for the next snapshot cycle.
                    // Must happen after the sync work above (this await yields).
                    let mut sb = pane_scrollback.write().await;
                    let buf = sb.entry(pane_id).or_insert_with(VecDeque::new);
                    for line in &clean_lines {
                        buf.push_back(line.to_string());
                        if buf.len() > 500 {
                            buf.pop_front();
                        }
                    }
                }

                panes_map.insert(
                    pane_id,
                    PaneEntry {
                        input_tx: handles.input_tx,
                        vt_parser: handles.parser,
                        master: handles.master,
                        child: handles.child,
                    },
                );
            }

            // Guard: a tab with no panes would panic in build_pane_layout.
            // Skip it gracefully — the rest of the snapshot is still usable.
            if pane_ids_in_tab.is_empty() {
                tracing::warn!(
                    "skipping tab {} ({}) in snapshot: no panes",
                    tab_snap.id,
                    tab_snap.name
                );
                continue;
            }

            let active_pane = PaneId(tab_snap.active_pane_id);
            let layout = tab_snap.layout.clone();
            tabs_map.insert(
                tab_id,
                TabState {
                    name: tab_snap.name.clone(),
                    layout,
                    active_pane,
                },
            );
            tab_order.push(tab_id);
        }

        // Fall back to first tab if the saved active_tab_id is no longer present.
        let active_tab = {
            let saved = TabId(snap.active_tab_id);
            if tab_order.contains(&saved) {
                saved
            } else {
                tab_order.first().copied().unwrap_or(TabId(u32::MAX))
            }
        };

        // Schedule agent command re-execution (1 s delay so the shell is ready).
        for agent_snap in &snap.agents {
            if let Some(ref cmd) = agent_snap.launch_cmd {
                // Use the first pane of the active tab as the target.
                let target_pane = tabs_map
                    .get(&active_tab)
                    .and_then(|t| t.layout.leaves().first().copied());
                if let Some(pane_id) = target_pane {
                    agent_registry
                        .set_pending_launch_cmd(pane_id, cmd.clone())
                        .await;
                    if let Some(entry) = panes_map.get(&pane_id) {
                        let input_tx = entry.input_tx.clone();
                        let cmd_bytes = format!("{}\r", cmd.trim()).into_bytes();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            let _ = input_tx.send(cmd_bytes).await;
                        });
                    }
                }
            }
        }

        Ok(Self {
            space_id,
            space_name: snap.name.clone(),
            panes: RwLock::new(panes_map),
            tabs: RwLock::new(tabs_map),
            tab_order: RwLock::new(tab_order),
            active_tab: RwLock::new(active_tab),
            next_pane_id,
            next_tab_id,
            event_bus,
            shell,
            cwd: snap.cwd.clone(),
            agent_registry,
            pane_scrollback,
        })
    }

    pub async fn collect_space_info(&self) -> SpaceInfo {
        let tabs = self.tabs.read().await;
        let tab_order = self.tab_order.read().await;
        let active_tab = *self.active_tab.read().await;
        let panes = self.panes.read().await;

        let mut all_pane_ids = Vec::new();
        let mut tab_infos = Vec::new();
        for tab_id in tab_order.iter() {
            if let Some(tab) = tabs.get(tab_id) {
                all_pane_ids.push((*tab_id, tab.layout.leaves()));
                tab_infos.push(TabInfo {
                    id: *tab_id,
                    name: tab.name.clone(),
                    layout: tab.layout.clone(),
                    active_pane: tab.active_pane,
                });
            }
        }

        // Determine the active pane for the active tab so we can read its live cwd.
        let active_pane_id = tabs.get(&active_tab).map(|t| t.active_pane).or_else(|| {
            all_pane_ids
                .first()
                .and_then(|(_, leaves)| leaves.first().copied())
        });

        let mut pane_infos: Vec<PaneInfo> = Vec::new();
        for (tab_id, leaves) in &all_pane_ids {
            for &pid in leaves {
                if let Some(entry) = panes.get(&pid) {
                    // Read live cwd from the child process; fall back to session cwd.
                    let child_pid = entry.child.lock().ok().and_then(|c| c.process_id());
                    let pane_cwd = child_pid
                        .map(|p| proc_cwd(p, &self.cwd))
                        .unwrap_or_else(|| self.cwd.clone());

                    let g = entry.vt_parser.lock().unwrap();
                    let grid = &g.grid;
                    pane_infos.push(PaneInfo {
                        id: pid,
                        tab_id: *tab_id,
                        title: "shell".to_string(),
                        cwd: pane_cwd,
                        cell_grid: CellGrid {
                            cols: grid.cols,
                            rows: grid.rows,
                            cells: grid.cells.clone(),
                            cursor_x: grid.cursor_x,
                            cursor_y: grid.cursor_y,
                            cursor_visible: grid.cursor_visible,
                            mouse_reporting: grid.mouse_reporting,
                            mouse_sgr: grid.mouse_sgr,
                        },
                    });
                }
            }
        }

        // Space-level path = live cwd of the active pane (shown in sidebar card + status bar).
        let space_path = active_pane_id
            .and_then(|pid| panes.get(&pid))
            .and_then(|entry| entry.child.lock().ok().and_then(|c| c.process_id()))
            .map(|p| proc_cwd(p, &self.cwd))
            .unwrap_or_else(|| self.cwd.clone());

        SpaceInfo {
            id: self.space_id,
            name: self.space_name.clone(),
            path: space_path,
            tabs: tab_infos,
            active_tab,
            panes: pane_infos,
        }
    }
}

/// Manages multiple spaces (sessions) with shared pane/tab ID counters.
pub struct SpaceManager {
    spaces: RwLock<HashMap<SpaceId, Arc<SessionState>>>,
    space_order: RwLock<Vec<SpaceId>>,
    active_space: RwLock<SpaceId>,
    next_space_id: AtomicU32,
    next_pane_id: Arc<AtomicU32>,
    next_tab_id: Arc<AtomicU32>,
    pub event_bus: broadcast::Sender<ServerEvent>,
    pub agent_registry: Arc<AgentRegistry>,
    shell: String,
    cwd: String,
}

impl SpaceManager {
    pub async fn new(
        event_bus: broadcast::Sender<ServerEvent>,
        shell: String,
        cwd: String,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<Self> {
        let next_space_id = AtomicU32::new(0);
        let next_pane_id = Arc::new(AtomicU32::new(0));
        let next_tab_id = Arc::new(AtomicU32::new(0));
        let agent_registry = AgentRegistry::new(event_bus.clone());

        let space_id = SpaceId(next_space_id.fetch_add(1, Ordering::Relaxed));
        let space_name = generate_space_name(&[]);

        let session = Arc::new(
            SessionState::with_counters(
                event_bus.clone(),
                shell.clone(),
                cwd.clone(),
                cols,
                rows,
                space_id,
                space_name,
                Arc::clone(&next_pane_id),
                Arc::clone(&next_tab_id),
                Arc::clone(&agent_registry),
            )
            .await?,
        );
        Arc::clone(&session).spawn_scrollback_collector();

        #[cfg(target_os = "linux")]
        Arc::clone(&agent_registry).spawn_global_scanner(space_id);

        let mut spaces = HashMap::new();
        spaces.insert(space_id, session);

        Ok(Self {
            spaces: RwLock::new(spaces),
            space_order: RwLock::new(vec![space_id]),
            active_space: RwLock::new(space_id),
            next_space_id,
            next_pane_id,
            next_tab_id,
            event_bus,
            agent_registry,
            shell,
            cwd,
        })
    }

    /// Create a `SpaceManager` with no initial spaces.
    /// Used when restoring from a snapshot: the caller populates spaces via `try_restore`.
    pub fn new_empty(
        event_bus: broadcast::Sender<ServerEvent>,
        shell: String,
        cwd: String,
    ) -> Self {
        let agent_registry = AgentRegistry::new(event_bus.clone());
        #[cfg(target_os = "linux")]
        Arc::clone(&agent_registry).spawn_global_scanner(SpaceId(0));
        Self {
            spaces: RwLock::new(HashMap::new()),
            space_order: RwLock::new(Vec::new()),
            active_space: RwLock::new(SpaceId(u32::MAX)),
            next_space_id: AtomicU32::new(0),
            next_pane_id: Arc::new(AtomicU32::new(0)),
            next_tab_id: Arc::new(AtomicU32::new(0)),
            event_bus,
            agent_registry,
            shell,
            cwd,
        }
    }

    /// Restore spaces from a `SessionSnapshot` saved at shutdown.
    /// Advances all ID counters past the saved IDs, spawns PTY shells at saved cwds,
    /// replays scrollback, and re-launches agent commands with a 1-second delay.
    /// Returns an error (and the caller falls back to a fresh start) if the snapshot
    /// is empty or any PTY spawn fails.
    pub async fn try_restore(
        &self,
        snap: &crate::daemon::snapshot::SessionSnapshot,
    ) -> anyhow::Result<()> {
        if snap.spaces.is_empty() {
            anyhow::bail!("snapshot contains no spaces — starting fresh");
        }

        for space_snap in &snap.spaces {
            let space_id = SpaceId(space_snap.id);

            // Advance all global counters past every ID in this snapshot so that
            // any new space/tab/pane created after restore cannot collide.
            self.next_space_id
                .fetch_max(space_snap.id + 1, Ordering::SeqCst);
            for tab_snap in &space_snap.tabs {
                self.next_tab_id
                    .fetch_max(tab_snap.id + 1, Ordering::SeqCst);
                for pane_snap in &tab_snap.panes {
                    self.next_pane_id
                        .fetch_max(pane_snap.id + 1, Ordering::SeqCst);
                }
            }

            let session = Arc::new(
                match SessionState::restore_from_snapshot(
                    space_snap,
                    self.event_bus.clone(),
                    Arc::clone(&self.agent_registry),
                    Arc::clone(&self.next_pane_id),
                    Arc::clone(&self.next_tab_id),
                    self.shell.clone(),
                )
                .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        // Shut down any PTYs that were already restored before propagating.
                        self.shutdown_all().await;
                        return Err(e);
                    }
                },
            );
            Arc::clone(&session).spawn_scrollback_collector();

            {
                self.spaces.write().await.insert(space_id, session);
            }
            {
                self.space_order.write().await.push(space_id);
            }
        }

        // Restore active space; fall back to the first restored space if the saved
        // ID is not present (e.g. partial snapshot or ID mismatch).
        let desired = SpaceId(snap.active_space_id);
        let exists = self.spaces.read().await.contains_key(&desired);
        *self.active_space.write().await = if exists {
            desired
        } else {
            self.space_order
                .read()
                .await
                .first()
                .copied()
                .unwrap_or(SpaceId(u32::MAX))
        };

        Ok(())
    }

    pub async fn active_session(&self) -> Arc<SessionState> {
        let active = *self.active_space.read().await;
        let spaces = self.spaces.read().await;
        spaces
            .get(&active)
            .expect("active space must exist")
            .clone()
    }

    pub async fn create_space(&self, name: Option<String>) -> anyhow::Result<SpaceId> {
        let space_id = SpaceId(self.next_space_id.fetch_add(1, Ordering::Relaxed));

        let existing_names: Vec<String> = {
            let spaces = self.spaces.read().await;
            let order = self.space_order.read().await;
            order
                .iter()
                .filter_map(|id| spaces.get(id))
                .map(|s| s.space_name.clone())
                .collect()
        };
        let name_refs: Vec<&str> = existing_names.iter().map(|s| s.as_str()).collect();
        let space_name = name.unwrap_or_else(|| generate_space_name(&name_refs));

        let session = Arc::new(
            SessionState::with_counters(
                self.event_bus.clone(),
                self.shell.clone(),
                self.cwd.clone(),
                80,
                24,
                space_id,
                space_name,
                Arc::clone(&self.next_pane_id),
                Arc::clone(&self.next_tab_id),
                Arc::clone(&self.agent_registry),
            )
            .await?,
        );
        Arc::clone(&session).spawn_scrollback_collector();

        {
            let mut spaces = self.spaces.write().await;
            spaces.insert(space_id, session);
        }
        {
            let mut order = self.space_order.write().await;
            order.push(space_id);
        }

        let space_info = {
            let spaces = self.spaces.read().await;
            let session = spaces.get(&space_id).unwrap();
            session.collect_space_info().await
        };
        let _ = self.event_bus.send(ServerEvent::SpaceCreated(space_info));

        {
            let mut active = self.active_space.write().await;
            *active = space_id;
        }

        let session = self.active_session().await;
        let info = session.collect_space_info().await;
        let _ = self.event_bus.send(ServerEvent::SpaceUpdated(info));

        Ok(space_id)
    }

    pub async fn close_space(&self, space_id: SpaceId) -> anyhow::Result<()> {
        let session = {
            let mut spaces = self.spaces.write().await;
            spaces.remove(&space_id)
        };
        if session.is_none() {
            anyhow::bail!("space not found: {:?}", space_id);
        }
        // Kill all PTYs in the removed session
        if let Some(sess) = session {
            let panes = sess.panes.write().await;
            for entry in panes.values() {
                if let Ok(mut child) = entry.child.lock() {
                    let _ = child.kill();
                }
            }
        }
        {
            let mut order = self.space_order.write().await;
            order.retain(|&id| id != space_id);
        }
        // If this was the active space, switch to the first remaining one
        {
            let mut active = self.active_space.write().await;
            if *active == space_id {
                let order = self.space_order.read().await;
                *active = order.first().copied().unwrap_or(SpaceId(u32::MAX));
            }
        }
        let _ = self.event_bus.send(ServerEvent::SpaceClosed(space_id));
        Ok(())
    }

    pub async fn reorder_space(&self, space_id: SpaceId, to_index: usize) {
        let mut order = self.space_order.write().await;
        if let Some(from) = order.iter().position(|&id| id == space_id) {
            order.remove(from);
            let clamped = to_index.min(order.len());
            order.insert(clamped, space_id);
        }
    }

    pub async fn switch_space(&self, space_id: SpaceId) -> anyhow::Result<()> {
        {
            let spaces = self.spaces.read().await;
            if !spaces.contains_key(&space_id) {
                anyhow::bail!("space not found: {:?}", space_id);
            }
        }
        {
            let mut active = self.active_space.write().await;
            *active = space_id;
        }
        let session = self.active_session().await;
        let info = session.collect_space_info().await;
        let _ = self.event_bus.send(ServerEvent::SpaceUpdated(info));
        Ok(())
    }

    /// Find the session that owns `pane_id`, searching all spaces.
    pub async fn get_session_for_pane(&self, pane_id: PaneId) -> Option<Arc<SessionState>> {
        let sessions: Vec<Arc<SessionState>> = {
            let spaces = self.spaces.read().await;
            spaces.values().cloned().collect()
        };
        for session in sessions {
            if session.panes.read().await.contains_key(&pane_id) {
                return Some(session);
            }
        }
        None
    }

    pub async fn nudge_all_spaces(&self) {
        let spaces = self.spaces.read().await;
        for session in spaces.values() {
            session.nudge_all_panes().await;
        }
    }

    /// Serialize all spaces to `~/.orbt/sessions/session.toml`.
    /// Drops the spaces/order read locks before doing async snapshot work to avoid
    /// holding them across the awaits inside `to_snapshot` (see CLAUDE.md §9.9).
    pub async fn save_snapshot(&self) {
        // Collect Arc references first, then release the read locks.
        let sessions: Vec<Arc<SessionState>> = {
            let order = self.space_order.read().await;
            let spaces = self.spaces.read().await;
            order
                .iter()
                .filter_map(|id| spaces.get(id).cloned())
                .collect()
        };
        let active_space_id = self.active_space.read().await.0;

        let mut space_snaps = Vec::new();
        for sess in &sessions {
            space_snaps.push(sess.to_snapshot().await);
        }

        let snap = crate::daemon::snapshot::SessionSnapshot {
            spaces: space_snaps,
            active_space_id,
        };
        if let Err(e) = crate::daemon::snapshot::save(&snap) {
            tracing::warn!("failed to save session snapshot: {e:#}");
        }
    }

    /// Kill every PTY child in every space — called from the signal handler before exit.
    pub async fn shutdown_all(&self) {
        let spaces = self.spaces.read().await;
        for session in spaces.values() {
            let panes = session.panes.read().await;
            for entry in panes.values() {
                if let Ok(mut child) = entry.child.lock() {
                    let _ = child.kill();
                }
            }
        }
    }

    /// Background task: every `interval_ms` milliseconds, re-read the active pane's
    /// cwd for every space and broadcast SpaceUpdated if it changed.
    pub async fn poll_cwd_changes(&self, interval_ms: u64) {
        let mut last_cwds: HashMap<SpaceId, String> = HashMap::new();
        let dur = std::time::Duration::from_millis(interval_ms);
        loop {
            tokio::time::sleep(dur).await;
            let order = self.space_order.read().await;
            let spaces = self.spaces.read().await;
            for &sid in order.iter() {
                if let Some(session) = spaces.get(&sid) {
                    let info = session.collect_space_info().await;
                    let prev = last_cwds.get(&sid).map(|s| s.as_str()).unwrap_or("");
                    if info.path != prev {
                        last_cwds.insert(sid, info.path.clone());
                        let _ = self.event_bus.send(ServerEvent::SpaceUpdated(info));
                    }
                }
            }
        }
    }

    pub async fn collect_full_state(&self) -> FullState {
        let active = *self.active_space.read().await;
        let spaces = self.spaces.read().await;
        let order = self.space_order.read().await;
        let mut space_infos = Vec::new();
        for id in order.iter() {
            if let Some(session) = spaces.get(id) {
                space_infos.push(session.collect_space_info().await);
            }
        }
        FullState {
            spaces: space_infos,
            active_space: active,
            agents: self.agent_registry.get_agents().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn session_snapshot_restore_roundtrip() {
        use crate::daemon::snapshot::{PaneSnapshot, SessionSnapshot, SpaceSnapshot, TabSnapshot};
        use tokio::sync::broadcast;

        let (event_bus, _rx) = broadcast::channel(16);
        // Use a command that exits immediately so spawn_blocking PTY reader tasks
        // see EOF quickly and do not keep the tokio runtime alive after the test.
        // Locate `true` via `which` to handle non-FHS layouts (e.g. NixOS).
        let shell = std::process::Command::new("which")
            .arg("true")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/usr/bin/true".to_string());
        let cwd = std::env::temp_dir().to_string_lossy().to_string();

        let snap = SessionSnapshot {
            active_space_id: 7,
            spaces: vec![SpaceSnapshot {
                id: 7,
                name: "restored-space".to_string(),
                cwd: cwd.clone(),
                active_tab_id: 3,
                tabs: vec![TabSnapshot {
                    id: 3,
                    name: "main".to_string(),
                    active_pane_id: 5,
                    panes: vec![PaneSnapshot {
                        id: 5,
                        cwd: cwd.clone(),
                        scrollback: vec!["$ echo hello".to_string(), "hello".to_string()],
                    }],
                    layout: PaneLayout::Leaf(PaneId(5)),
                }],
                agents: vec![],
            }],
        };

        let sm = SpaceManager::new_empty(event_bus, shell, cwd);
        sm.try_restore(&snap).await.expect("restore must succeed");

        // Space was created with correct ID and name.
        {
            let spaces = sm.spaces.read().await;
            assert_eq!(spaces.len(), 1, "expected exactly 1 restored space");
            assert!(
                spaces.contains_key(&SpaceId(7)),
                "SpaceId(7) must be present"
            );
            let sess = spaces.get(&SpaceId(7)).unwrap();
            assert_eq!(sess.space_name, "restored-space");
        }

        // Space order tracks it.
        {
            let order = sm.space_order.read().await;
            assert_eq!(order.len(), 1);
            assert_eq!(order[0], SpaceId(7));
        }

        // Active space restored correctly.
        let active = *sm.active_space.read().await;
        assert_eq!(active, SpaceId(7));

        // ID counters must have been advanced past all saved IDs.
        let next_space = sm.next_space_id.load(Ordering::Relaxed);
        assert!(next_space >= 8, "next_space_id {next_space} must be > 7");
        let next_tab = sm.next_tab_id.load(Ordering::Relaxed);
        assert!(next_tab >= 4, "next_tab_id {next_tab} must be > 3");
        let next_pane = sm.next_pane_id.load(Ordering::Relaxed);
        assert!(next_pane >= 6, "next_pane_id {next_pane} must be > 5");

        // Kill PTY children so spawn_blocking reader tasks get EOF and exit,
        // allowing the tokio runtime to shut down cleanly after the test.
        sm.shutdown_all().await;
    }

    #[test]
    fn space_name_format() {
        let name = generate_space_name(&[]);
        let parts: Vec<&str> = name.splitn(2, '-').collect();
        assert_eq!(parts.len(), 2, "name should be adjective-noun: {name}");
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
    }

    #[test]
    fn space_name_avoids_duplicates() {
        // Fill up all 400 combinations by calling many times — just verify no panic
        let mut seen = vec![];
        for _ in 0..20 {
            let refs: Vec<&str> = seen.iter().map(|s: &String| s.as_str()).collect();
            let name = generate_space_name(&refs);
            seen.push(name);
        }
        assert_eq!(seen.len(), 20);
    }
}
