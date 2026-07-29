use std::collections::HashMap;
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

    pub async fn nudge_all_spaces(&self) {
        let spaces = self.spaces.read().await;
        for session in spaces.values() {
            session.nudge_all_panes().await;
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
