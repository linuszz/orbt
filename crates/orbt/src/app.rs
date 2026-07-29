use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use orbt_core::VtParser;
use orbt_protocol::{
    AgentId, AgentInfo, AgentMetrics, AgentStatus, Cell, CellGrid, FileTouched, FullState, PaneId,
    PaneLayout, ServerEvent, SpaceId, SplitDir, TabId, ToolCall,
};

/// Which column has keyboard focus in the mobile SPACES two-column view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MobileColFocus {
    #[default]
    Left,
    Right,
}

/// Mobile layout view tabs (bottom navigation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MobileView {
    /// Full-screen PTY terminal (default).
    #[default]
    Terminal,
    /// Agent Fleet list.
    Agents,
    /// Tab/space switcher.
    Windows,
    /// Command palette / settings.
    Actions,
}

impl MobileView {
    /// Cycle forward through the four views (TTY → WINDOWS → COMMAND → AGENTS → TTY).
    pub fn next(self) -> Self {
        match self {
            Self::Terminal => Self::Windows,
            Self::Windows => Self::Actions,
            Self::Actions => Self::Agents,
            Self::Agents => Self::Terminal,
        }
    }

    /// Cycle backward through the four views.
    pub fn prev(self) -> Self {
        match self {
            Self::Terminal => Self::Agents,
            Self::Agents => Self::Actions,
            Self::Actions => Self::Windows,
            Self::Windows => Self::Terminal,
        }
    }
}

/// Identifies what is being closed in a mobile close-confirmation dialog.
#[derive(Debug, Clone)]
pub enum MobileCloseTarget {
    Space(usize),
    Tab(usize),
}

/// State for the mobile close-confirmation modal.
#[derive(Debug, Clone)]
pub struct MobileCloseConfirm {
    pub target: MobileCloseTarget,
    /// true = Confirm button focused; false = Cancel button focused (safe default).
    pub confirm_focused: bool,
}

/// How the Agent Fleet panel is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentPanelMode {
    /// Hidden — panel not shown.
    #[default]
    Hidden,
    /// Sidebar — panel occupies a fixed column on the right side of the layout.
    Sidebar,
    /// Modal — panel floats as a centered overlay over the main pane area.
    Modal,
}

impl AgentPanelMode {
    /// Cycle: Hidden → Sidebar → Modal → Hidden.
    pub fn cycle(self) -> Self {
        match self {
            Self::Hidden => Self::Sidebar,
            Self::Sidebar => Self::Modal,
            Self::Modal => Self::Hidden,
        }
    }

    pub fn is_visible(self) -> bool {
        !matches!(self, Self::Hidden)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct UserSettings {
    pub theme: String,
    pub sidebar_visible: bool,
    #[serde(default)]
    pub agent_panel_mode: AgentPanelMode,
    /// Legacy field — migrated to agent_panel_mode on load.
    #[serde(default, skip_serializing)]
    pub agent_panel_visible: bool,
    /// Hidden opt-in switch. Set to `true` in settings.toml to enable the
    /// Agent Fleet panel, mobile Agents tab, and all related UI. Defaults to
    /// false — the feature is not shown until explicitly opted in.
    #[serde(default)]
    pub agent_fleet_enabled: bool,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            theme: "orbt".to_string(),
            sidebar_visible: true,
            agent_panel_mode: AgentPanelMode::Hidden,
            agent_panel_visible: false,
            agent_fleet_enabled: false,
        }
    }
}

fn settings_path() -> std::path::PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    base.join("orbt").join("settings.toml")
}

pub fn load_settings() -> UserSettings {
    let path = settings_path();
    let mut s: UserSettings = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();
    // Migrate legacy agent_panel_visible → agent_panel_mode.
    if s.agent_panel_mode == AgentPanelMode::Hidden && s.agent_panel_visible {
        s.agent_panel_mode = AgentPanelMode::Sidebar;
    }
    s
}

pub fn save_settings(app: &App) {
    let settings = UserSettings {
        theme: app.theme_name.clone(),
        sidebar_visible: app.sidebar_visible,
        agent_panel_mode: app.agent_panel_mode,
        agent_panel_visible: false,
        agent_fleet_enabled: app.agent_fleet_enabled,
    };
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = toml::to_string_pretty(&settings) {
        let _ = std::fs::write(path, s);
    }
}

// Fields consumed by Task 4 (sidebar rendering); suppressing dead_code until then.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SpaceEntry {
    pub space_id: SpaceId,
    pub name: String,
    pub cwd: String,
    pub tab_count: usize,
    pub pane_count: usize,
}

// Fields consumed by Tasks 4 and 5 (tab bar, mouse selection); suppressing dead_code until then.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Selection {
    pub pane_id: PaneId,
    pub start: (u16, u16), // (col, row) in cell coords within pane
    pub end: (u16, u16),
    pub active: bool,
}

/// Known agent types the user can launch from the Launch Agent overlay.
/// Fields: (command, display_name, acp_capable)
pub const LAUNCH_AGENTS: &[(&str, &str, bool)] = &[
    ("claude", "Claude Code", false),
    ("opencode", "OpenCode AI", true),
    ("codex-cli", "Codex CLI", true),
    ("gemini", "Gemini CLI", true),
    ("aider", "Aider", false),
    ("cline", "Cline", true),
];

/// Which field has keyboard focus in the Launch Agent modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchFocus {
    #[default]
    AgentList,
    Name,
    Model,
    Cwd,
}

impl LaunchFocus {
    pub fn next(self) -> Self {
        match self {
            Self::AgentList => Self::Name,
            Self::Name => Self::Model,
            Self::Model => Self::Cwd,
            Self::Cwd => Self::AgentList,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::AgentList => Self::Cwd,
            Self::Name => Self::AgentList,
            Self::Model => Self::Name,
            Self::Cwd => Self::Model,
        }
    }
}

/// State for the "Launch Agent" configuration overlay.
#[derive(Debug, Clone)]
pub struct LaunchModalState {
    pub selected_agent: usize,
    pub focus: LaunchFocus,
    pub name: String,
    pub model: String,
    pub cwd: String,
}

/// State for the Satellite Eclipse intervention modal.
#[derive(Debug, Clone)]
pub struct EclipseModalState {
    pub agent_id: AgentId,
    pub agent_name: String,
    pub block_msg: String,
    pub response: String,
    /// Snapshot of agent context captured at modal open time.
    pub model: String,
    pub task: Option<String>,
    pub progress: Option<f32>,
    pub cwd: Option<String>,
    /// Wall-clock seconds since agent was blocked (captured at open time).
    pub blocked_duration_s: u32,
}

#[derive(Debug, Clone)]
pub struct AgentDetailModalState {
    pub agent_id: AgentId,
    pub agent_name: String,
    pub status: AgentStatus,
    pub duration_s: u32,
    pub model: String,
    pub cwd: Option<String>,
    pub task: Option<String>,
    // ACP fields (zero/empty for heuristic agents)
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub current_tool: Option<ToolCall>,
    pub recent_tools: Vec<ToolCall>,
    pub total_tool_calls: u32,
    pub files_touched: Vec<FileTouched>,
    // UI scroll state
    pub tool_scroll: usize,
    /// Cached output lines from the most recent AgentMetricsUpdated event.
    pub recent_output_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    CommandPalette {
        search: String,
        selected: usize,
        search_focused: bool,
    },
    Scroll {
        offset: usize,
    },
    /// Keyboard navigation mode for the Agent Fleet panel (prefix+a).
    AgentPanel {
        selected: usize,
    },
}

pub struct CommandDef {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub shortcut: &'static str,
}

pub static COMMANDS: &[CommandDef] = &[
    // tmux: % = split horizontal (left|right), " = split vertical (top/bottom)
    CommandDef {
        id: "split_h",
        label: "Split Horizontal",
        group: "Pane",
        shortcut: "%",
    },
    CommandDef {
        id: "split_v",
        label: "Split Vertical",
        group: "Pane",
        shortcut: "\"",
    },
    CommandDef {
        id: "close_pane",
        label: "Close Pane",
        group: "Pane",
        shortcut: "x",
    },
    CommandDef {
        id: "cycle_pane",
        label: "Cycle Pane Focus",
        group: "Pane",
        shortcut: "o",
    },
    CommandDef {
        id: "zoom_pane",
        label: "Zoom Pane",
        group: "Pane",
        shortcut: "z",
    },
    CommandDef {
        id: "scroll_mode",
        label: "Enter Copy/Scroll Mode",
        group: "Pane",
        shortcut: "[",
    },
    // tmux: c = new window, n/p = next/prev, l = last
    CommandDef {
        id: "new_tab",
        label: "New Tab",
        group: "Tab",
        shortcut: "c",
    },
    CommandDef {
        id: "next_tab",
        label: "Next Tab",
        group: "Tab",
        shortcut: "n",
    },
    CommandDef {
        id: "prev_tab",
        label: "Previous Tab",
        group: "Tab",
        shortcut: "p",
    },
    // tmux: d = detach
    CommandDef {
        id: "detach",
        label: "Detach Session",
        group: "Session",
        shortcut: "d",
    },
    // Orbit extensions (not in tmux but useful)
    CommandDef {
        id: "toggle_sidebar",
        label: "Toggle Sidebar",
        group: "View",
        shortcut: "b",
    },
    CommandDef {
        id: "toggle_agent",
        label: "Toggle Agent Monitor",
        group: "View",
        shortcut: "a",
    },
    CommandDef {
        id: "toggle_theme",
        label: "Toggle Theme",
        group: "View",
        shortcut: "T",
    },
    CommandDef {
        id: "settings",
        label: "Settings",
        group: "View",
        shortcut: ",",
    },
    CommandDef {
        id: "help",
        label: "Show Help",
        group: "Help",
        shortcut: "?",
    },
    CommandDef {
        id: "agent_scroll_up",
        label: "Scroll Agent Fleet Up",
        group: "Satellite",
        shortcut: "k",
    },
    CommandDef {
        id: "agent_scroll_down",
        label: "Scroll Agent Fleet Down",
        group: "Satellite",
        shortcut: "j",
    },
    // Phase 3: paste local clipboard image into active pane as a file path
    CommandDef {
        id: "paste_image",
        label: "Paste Image from Clipboard",
        group: "Pane",
        shortcut: "I",
    },
];

pub struct PaneState {
    pub parser: VtParser,
    pub scrollback: VecDeque<Vec<Cell>>,
}

const SCROLLBACK_CAP: usize = 10_000;

impl PaneState {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            parser: VtParser::new(cols, rows),
            scrollback: VecDeque::with_capacity(SCROLLBACK_CAP),
        }
    }

    pub fn process(&mut self, data: &[u8]) {
        self.parser.process(data);
        for row in self.parser.grid.drain_scrolled_rows() {
            if self.scrollback.len() >= SCROLLBACK_CAP {
                self.scrollback.pop_front();
            }
            self.scrollback.push_back(row);
        }
    }

    pub fn sync_from_server(&mut self, grid: &CellGrid) {
        self.parser.grid.cols = grid.cols;
        self.parser.grid.rows = grid.rows;
        self.parser.grid.cells = grid.cells.clone();
        self.parser.grid.cursor_x = grid.cursor_x.min(grid.cols.saturating_sub(1));
        self.parser.grid.cursor_y = grid.cursor_y.min(grid.rows.saturating_sub(1));
        // Snapshot cursor_visible=false is unreliable: TUI apps hide the cursor during every
        // render cycle, so the snapshot often captures a mid-render state. Assume visible until
        // a live PaneOutput delivers ESC[?25l explicitly.
        self.parser.grid.cursor_visible = true;
        self.parser.grid.mouse_reporting = grid.mouse_reporting;
        self.parser.grid.mouse_sgr = grid.mouse_sgr;
        self.parser.grid.scroll_top = 0;
        self.parser.grid.scroll_bottom = grid.rows.saturating_sub(1);
        self.parser.reset_parser();
    }
}

#[derive(Debug, Clone)]
pub struct Tab {
    pub id: TabId,
    pub name: String,
    pub pane_tree: PaneLayout,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentHover {
    HeaderAdd,
    HeaderClose,
    EclipseRespond,
    CardBtn {
        card_idx: usize,
        slot: u8,
    },
    /// "[+] Add Satellite" footer button at the bottom of the panel.
    PanelFooter,
}

pub struct App {
    pub panes: HashMap<PaneId, PaneState>,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub active_tab_id: TabId,
    pub active_pane: PaneId,
    pub pending_split: Option<(PaneId, SplitDir)>,
    pub mode: InputMode,
    pub should_quit: bool,
    pub needs_redraw: bool,
    pub needs_resize: bool,
    pub server_connected: bool,
    pub sidebar_visible: bool,
    pub agent_panel_mode: AgentPanelMode,
    pub show_help: bool,
    pub context_menu: Option<ContextMenu>,
    pub space_name: String,
    pub space_path: String,
    pub spaces: Vec<SpaceEntry>,
    pub active_space_idx: usize,
    pub tab_hovered: Option<usize>,
    pub sidebar_hovered: Option<usize>,
    pub sidebar_toggle_hovered: bool,
    pub selection: Option<Selection>,
    pub agents: Vec<AgentInfo>,
    pub agent_metrics: HashMap<AgentId, AgentMetrics>,
    /// Client-side start times for smooth live duration display.
    pub agent_start_times: HashMap<AgentId, Instant>,
    /// Timestamps when each agent entered the Blocked state (for accurate "Blocked: Xm" display).
    pub agent_blocked_times: HashMap<AgentId, Instant>,
    pub agent_hovered: Option<AgentHover>,
    pub agent_scroll_offset: usize,
    pub eclipse_modal: Option<EclipseModalState>,
    pub agent_detail_modal: Option<AgentDetailModalState>,
    pub launch_modal: Option<LaunchModalState>,
    /// Increments every animation tick (16 ms) while any agent is Working or Blocked.
    pub tick_count: u64,
    pub drag_tab: Option<usize>,
    pub drag_split: Option<(PaneId, PaneId, SplitDir, f32)>,
    pub theme_name: String,
    /// Runtime copy of `UserSettings::agent_fleet_enabled`. Set at startup, fixed for the session.
    pub agent_fleet_enabled: bool,
    pub settings_open: bool,
    pub settings_selected: usize,
    /// Set when orbtd acknowledges an UploadPayload with the remote path.
    /// events.rs drains this and injects the path as PTY input.
    pub pending_payload_path: Option<String>,
    /// True when terminal dimensions trigger mobile mode (cols < 80 || rows < 25).
    pub mobile_mode: bool,
    /// Active view in mobile bottom-nav layout.
    pub mobile_view: MobileView,
    /// Cursor row in the left (Spaces) column of the SPACES view.
    pub mobile_spaces_cursor: usize,
    /// Cursor row in the right (Tabs) column of the SPACES view.
    pub mobile_tabs_cursor: usize,
    /// Which column has keyboard focus in the SPACES view.
    pub mobile_col_focus: MobileColFocus,
    /// Pending close confirmation modal state (mobile SPACES view).
    pub mobile_close_confirm: Option<MobileCloseConfirm>,
}

#[derive(Debug, Clone)]
pub enum ContextMenuTarget {
    Pane(PaneId),
    Space(SpaceId),
    Sidebar,
    Tab(TabId),
}

#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub x: u16,
    pub y: u16,
    pub target: ContextMenuTarget,
    pub items: Vec<ContextMenuItem>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub enum ContextMenuItem {
    Action {
        label: String,
        shortcut: String,
        id: &'static str,
    },
    Separator,
}

impl App {
    pub fn from_welcome(state: &FullState, cols: u16, rows: u16) -> Self {
        let spaces: Vec<SpaceEntry> = state
            .spaces
            .iter()
            .map(|s| SpaceEntry {
                space_id: s.id,
                name: s.name.clone(),
                cwd: s.path.clone(),
                tab_count: s.tabs.len(),
                pane_count: s.panes.len(),
            })
            .collect();

        let active_space_idx = state
            .spaces
            .iter()
            .position(|s| s.id == state.active_space)
            .unwrap_or(0);

        let space = state
            .spaces
            .iter()
            .find(|s| s.id == state.active_space)
            .or_else(|| state.spaces.first());
        let mut panes = HashMap::new();
        let mut tabs = Vec::new();
        let mut active_tab_idx = 0;
        let mut active_tab_id = TabId(0);
        let mut active_pane = PaneId(0);

        if let Some(s) = space {
            for pane in &s.panes {
                let mut ps = PaneState::new(pane.cell_grid.cols.max(1), pane.cell_grid.rows.max(1));
                ps.parser.grid.cells = pane.cell_grid.cells.clone();
                ps.parser.grid.cursor_x = pane.cell_grid.cursor_x;
                ps.parser.grid.cursor_y = pane.cell_grid.cursor_y;
                // See sync_from_server: snapshot cursor_visible=false is unreliable.
                ps.parser.grid.cursor_visible = true;
                ps.parser.grid.mouse_reporting = pane.cell_grid.mouse_reporting;
                ps.parser.grid.mouse_sgr = pane.cell_grid.mouse_sgr;
                ps.parser.grid.resize(cols, rows);
                panes.insert(pane.id, ps);
            }

            for (i, tab_info) in s.tabs.iter().enumerate() {
                tabs.push(Tab {
                    id: tab_info.id,
                    name: tab_info.name.clone(),
                    pane_tree: tab_info.layout.clone(),
                });
                if tab_info.id == s.active_tab {
                    active_tab_idx = i;
                    active_tab_id = tab_info.id;
                    active_pane = tab_info.active_pane;
                }
            }
        }

        let first_pane = space
            .and_then(|s| s.panes.first())
            .map(|p| p.id)
            .unwrap_or(PaneId(0));

        if tabs.is_empty() {
            let pane_tree = space
                .and_then(|s| s.tabs.first().map(|t| t.layout.clone()))
                .unwrap_or(PaneLayout::Leaf(first_pane));
            tabs.push(Tab {
                id: TabId(0),
                name: "dev".to_string(),
                pane_tree,
            });
        }

        Self {
            panes,
            tabs,
            active_tab: active_tab_idx,
            active_tab_id,
            active_pane,
            pending_split: None,
            mode: InputMode::Normal,
            should_quit: false,
            needs_redraw: true,
            needs_resize: false,
            server_connected: true,
            sidebar_visible: true,
            agent_panel_mode: AgentPanelMode::Hidden,
            show_help: false,
            context_menu: None,
            space_name: spaces
                .get(active_space_idx)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "orbt".to_string()),
            space_path: spaces
                .get(active_space_idx)
                .map(|s| s.cwd.clone())
                .unwrap_or_else(|| ".".to_string()),
            spaces,
            active_space_idx,
            tab_hovered: None,
            sidebar_hovered: None,
            sidebar_toggle_hovered: false,
            selection: None,
            agents: state.agents.clone(),
            agent_metrics: HashMap::new(),
            agent_start_times: {
                let mut m = HashMap::new();
                for a in &state.agents {
                    let duration_s = a.detail.as_ref().map(|d| d.duration_s).unwrap_or(0);
                    m.insert(
                        a.id,
                        Instant::now() - std::time::Duration::from_secs(duration_s as u64),
                    );
                }
                m
            },
            agent_blocked_times: {
                let mut m = HashMap::new();
                for a in &state.agents {
                    if a.status == AgentStatus::Blocked {
                        let duration_s = a.detail.as_ref().map(|d| d.duration_s).unwrap_or(0);
                        m.insert(
                            a.id,
                            Instant::now() - std::time::Duration::from_secs(duration_s as u64),
                        );
                    }
                }
                m
            },
            agent_hovered: None,
            agent_scroll_offset: 0,
            eclipse_modal: None,
            agent_detail_modal: None,
            launch_modal: None,
            tick_count: 0,
            drag_tab: None,
            drag_split: None,
            theme_name: "orbt".to_string(),
            agent_fleet_enabled: false,
            settings_open: false,
            settings_selected: 0,
            pending_payload_path: None,
            mobile_mode: cols < 80 || rows < 25,
            mobile_view: MobileView::Terminal,
            mobile_spaces_cursor: 0,
            mobile_tabs_cursor: 0,
            mobile_col_focus: MobileColFocus::Left,
            mobile_close_confirm: None,
        }
    }

    /// Sort agents: Blocked first, then Working, then Error, then Idle/Done.
    pub fn sort_agents(&mut self) {
        // Save the currently-selected agent's ID so the cursor follows it through the sort.
        let selected_id = if let InputMode::AgentPanel { selected } = self.mode {
            self.agents.get(selected).map(|a| a.id)
        } else {
            None
        };

        let order_before: Vec<AgentId> = self.agents.iter().map(|a| a.id).collect();
        self.agents.sort_by_key(|a| match a.status {
            AgentStatus::Blocked => 0u8,
            AgentStatus::Working => 1,
            AgentStatus::Error => 2,
            AgentStatus::Idle => 3,
            AgentStatus::Done => 4,
        });
        // Only reset hover if card positions actually changed; avoids hover flicker.
        let order_after: Vec<AgentId> = self.agents.iter().map(|a| a.id).collect();
        if order_before != order_after {
            self.agent_hovered = None;
            // Repoint `selected` to the same agent at its new position.
            if let (InputMode::AgentPanel { selected }, Some(id)) = (&mut self.mode, selected_id) {
                if let Some(new_pos) = self.agents.iter().position(|a| a.id == id) {
                    *selected = new_pos;
                    // Keep scroll clamped so the selected card stays on-screen.
                    self.agent_scroll_offset = self.agent_scroll_offset.min(new_pos);
                }
            }
        }
    }

    /// True when the animation tick timer should be active (Working/Blocked pulse + Error blink).
    pub fn has_active_agents(&self) -> bool {
        self.agents.iter().any(|a| {
            matches!(
                a.status,
                AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Error
            )
        })
    }

    pub fn pane_tree(&self) -> &PaneLayout {
        &self.tabs[self.active_tab].pane_tree
    }

    pub fn current_tab_name(&self) -> &str {
        &self.tabs[self.active_tab].name
    }

    pub fn cycle_focus(&mut self) {
        let leaves = self.pane_tree().leaves();
        if leaves.len() < 2 {
            return;
        }
        let idx = leaves
            .iter()
            .position(|&p| p == self.active_pane)
            .unwrap_or(0);
        self.active_pane = leaves[(idx + 1) % leaves.len()];
        self.needs_redraw = true;
    }

    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.selection = None;
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
            self.active_tab_id = self.tabs[self.active_tab].id;
            let leaves = self.pane_tree().leaves();
            if let Some(&first) = leaves.first() {
                self.active_pane = first;
            }
            self.needs_redraw = true;
        }
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.selection = None;
            self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
            self.active_tab_id = self.tabs[self.active_tab].id;
            let leaves = self.pane_tree().leaves();
            if let Some(&first) = leaves.first() {
                self.active_pane = first;
            }
            self.needs_redraw = true;
        }
    }

    pub fn pane_in_current_tab(&self, pane_id: PaneId) -> bool {
        self.pane_tree().leaves().contains(&pane_id)
    }

    pub fn open_context_menu(&mut self, x: u16, y: u16, target: ContextMenuTarget) {
        let items = match &target {
            ContextMenuTarget::Pane(pane_id) => {
                let mut items = vec![
                    ContextMenuItem::Action {
                        label: "Split Horizontal".into(),
                        shortcut: "h".into(),
                        id: "split_h",
                    },
                    ContextMenuItem::Action {
                        label: "Split Vertical".into(),
                        shortcut: "v".into(),
                        id: "split_v",
                    },
                    ContextMenuItem::Separator,
                    ContextMenuItem::Action {
                        label: "Close Pane".into(),
                        shortcut: "x".into(),
                        id: "close_pane",
                    },
                    ContextMenuItem::Action {
                        label: "Maximize Pane".into(),
                        shortcut: "z".into(),
                        id: "maximize",
                    },
                ];
                if self
                    .selection
                    .as_ref()
                    .is_some_and(|s| s.pane_id == *pane_id)
                {
                    items.insert(
                        0,
                        ContextMenuItem::Action {
                            label: "Copy Selection".into(),
                            shortcut: String::new(),
                            id: "copy_selection",
                        },
                    );
                    items.insert(1, ContextMenuItem::Separator);
                }
                items
            }
            ContextMenuTarget::Space(_space_id) => vec![
                ContextMenuItem::Action {
                    label: "Rename".into(),
                    shortcut: "r".into(),
                    id: "rename_space",
                },
                ContextMenuItem::Separator,
                ContextMenuItem::Action {
                    label: "Move Up".into(),
                    shortcut: "".into(),
                    id: "move_up",
                },
                ContextMenuItem::Action {
                    label: "Move Down".into(),
                    shortcut: "".into(),
                    id: "move_down",
                },
                ContextMenuItem::Separator,
                ContextMenuItem::Action {
                    label: "Close".into(),
                    shortcut: "".into(),
                    id: "close_space",
                },
            ],
            ContextMenuTarget::Sidebar => vec![ContextMenuItem::Action {
                label: "New Space".into(),
                shortcut: "".into(),
                id: "new_space",
            }],
            ContextMenuTarget::Tab(_tab_id) => vec![
                ContextMenuItem::Action {
                    label: "New Tab".into(),
                    shortcut: "c".into(),
                    id: "new_tab",
                },
                ContextMenuItem::Separator,
                ContextMenuItem::Action {
                    label: "Close Tab".into(),
                    shortcut: "x".into(),
                    id: "close_tab",
                },
            ],
        };
        self.context_menu = Some(ContextMenu {
            x,
            y,
            target,
            items,
            selected: 0,
        });
        self.needs_redraw = true;
    }

    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
        self.needs_redraw = true;
    }

    pub fn handle_server_event(&mut self, event: &ServerEvent) {
        match event {
            ServerEvent::Welcome { state, .. } => {
                let active_space = state
                    .spaces
                    .iter()
                    .find(|s| s.id == state.active_space)
                    .or_else(|| state.spaces.first());
                if let Some(s) = active_space {
                    for pane in &s.panes {
                        if let Some(existing) = self.panes.get_mut(&pane.id) {
                            existing.sync_from_server(&pane.cell_grid);
                        } else {
                            let mut ps = PaneState::new(
                                pane.cell_grid.cols.max(1),
                                pane.cell_grid.rows.max(1),
                            );
                            ps.sync_from_server(&pane.cell_grid);
                            self.panes.insert(pane.id, ps);
                        }
                    }
                    if let Some(active_tab_info) = s.tabs.iter().find(|t| t.id == s.active_tab) {
                        self.active_pane = active_tab_info.active_pane;
                    }
                    self.space_name = s.name.clone();
                    self.space_path = s.path.clone();

                    let mut new_tabs = Vec::new();
                    let mut new_active_idx = 0;
                    let mut found_active = false;
                    for (i, tab_info) in s.tabs.iter().enumerate() {
                        new_tabs.push(Tab {
                            id: tab_info.id,
                            name: tab_info.name.clone(),
                            pane_tree: tab_info.layout.clone(),
                        });
                        if tab_info.id == s.active_tab {
                            new_active_idx = i;
                            found_active = true;
                        }
                    }
                    if !new_tabs.is_empty() {
                        self.tabs = new_tabs;
                        self.active_tab = if found_active { new_active_idx } else { 0 };
                        self.active_tab_id = s.active_tab;
                    }
                }
                self.spaces = state
                    .spaces
                    .iter()
                    .map(|s| SpaceEntry {
                        space_id: s.id,
                        name: s.name.clone(),
                        cwd: s.path.clone(),
                        tab_count: s.tabs.len(),
                        pane_count: s.panes.len(),
                    })
                    .collect();
                self.active_space_idx = state
                    .spaces
                    .iter()
                    .position(|s| s.id == state.active_space)
                    .unwrap_or(0);
                self.agents = state.agents.clone();
                // Re-seed start times for agents from the reconnected state.
                for a in &state.agents {
                    self.agent_start_times.entry(a.id).or_insert_with(|| {
                        let duration_s = a.detail.as_ref().map(|d| d.duration_s).unwrap_or(0);
                        Instant::now() - std::time::Duration::from_secs(duration_s as u64)
                    });
                }
                self.agent_start_times
                    .retain(|id, _| state.agents.iter().any(|a| a.id == *id));
                self.agent_metrics
                    .retain(|id, _| state.agents.iter().any(|a| a.id == *id));
                self.sort_agents();
                self.needs_redraw = true;
            }
            ServerEvent::PaneOutput { pane_id, data } => {
                if let Some(sel) = &self.selection {
                    if sel.pane_id == *pane_id {
                        self.selection = None;
                    }
                }
                if let Some(pane) = self.panes.get_mut(pane_id) {
                    pane.process(data);
                }
                if self.pane_in_current_tab(*pane_id) {
                    self.needs_redraw = true;
                }
            }
            ServerEvent::SpaceUpdated(info) => {
                let old_ids: std::collections::HashSet<PaneId> =
                    self.panes.keys().copied().collect();
                let new_ids: std::collections::HashSet<PaneId> =
                    info.panes.iter().map(|p| p.id).collect();

                for pane in &info.panes {
                    if old_ids.contains(&pane.id) {
                        // For existing panes the client VT is authoritative for cell content
                        // and cursor state (updated live via PaneOutput). Only sync grid
                        // dimensions so a resize is reflected without clobbering VT state.
                        if let Some(existing) = self.panes.get_mut(&pane.id) {
                            let new_cols = pane.cell_grid.cols.max(1);
                            let new_rows = pane.cell_grid.rows.max(1);
                            if existing.parser.grid.cols != new_cols
                                || existing.parser.grid.rows != new_rows
                            {
                                existing.parser.grid.resize(new_cols, new_rows);
                            }
                        }
                    } else {
                        let mut ps =
                            PaneState::new(pane.cell_grid.cols.max(1), pane.cell_grid.rows.max(1));
                        ps.sync_from_server(&pane.cell_grid);
                        self.panes.insert(pane.id, ps);

                        if let Some((target, dir)) = self.pending_split.take() {
                            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                                tab.pane_tree.split_leaf(target, dir, pane.id);
                            }
                            self.active_pane = pane.id;
                        }
                    }
                }

                for &id in old_ids.iter() {
                    if !new_ids.contains(&id) {
                        self.panes.remove(&id);
                    }
                }

                let mut new_tabs = Vec::new();
                let mut new_active_idx = 0;
                let mut found_active = false;
                let mut matched_active_tab_id = info.active_tab;
                for (i, tab_info) in info.tabs.iter().enumerate() {
                    new_tabs.push(Tab {
                        id: tab_info.id,
                        name: tab_info.name.clone(),
                        pane_tree: tab_info.layout.clone(),
                    });
                    if tab_info.id == info.active_tab {
                        new_active_idx = i;
                        found_active = true;
                        matched_active_tab_id = tab_info.id;
                    }
                }
                if !new_tabs.is_empty() {
                    let prev_active_tab_id = self.active_tab_id;
                    self.tabs = new_tabs;
                    self.active_tab = if found_active { new_active_idx } else { 0 };
                    self.active_tab_id = matched_active_tab_id;
                    let tab_changed = prev_active_tab_id != self.active_tab_id
                        || !self.panes.contains_key(&self.active_pane);
                    if tab_changed {
                        if let Some(active_tab) = self.tabs.get(self.active_tab) {
                            let server_active = info
                                .tabs
                                .iter()
                                .find(|t| t.id == self.active_tab_id)
                                .map(|t| t.active_pane);
                            self.active_pane = server_active
                                .filter(|&pid| self.panes.contains_key(&pid))
                                .or_else(|| active_tab.pane_tree.leaves().first().copied())
                                .unwrap_or(self.active_pane);
                        }
                    }
                }

                if self.tabs.is_empty() {
                    eprintln!("Exiting: SpaceUpdated with empty tabs");
                    self.should_quit = true;
                }

                // Update space-level name/path shown in the sidebar card and status bar.
                self.space_name = info.name.clone();
                self.space_path = info.path.clone();
                // Also update the matching SpaceEntry in the sidebar list.
                if let Some(entry) = self.spaces.iter_mut().find(|s| s.space_id == info.id) {
                    entry.cwd = info.path.clone();
                    entry.tab_count = info.tabs.len();
                    entry.pane_count = info.panes.len();
                }

                self.needs_redraw = true;
                self.needs_resize = true;
            }
            ServerEvent::SpaceCreated(info) => {
                self.spaces.push(SpaceEntry {
                    space_id: info.id,
                    name: info.name.clone(),
                    cwd: info.path.clone(),
                    tab_count: info.tabs.len(),
                    pane_count: info.panes.len(),
                });
                self.active_space_idx = self.spaces.len() - 1;
                self.needs_redraw = true;
            }
            ServerEvent::SpaceClosed(closed_id) => {
                // Remove the closed space from the sidebar list.
                self.spaces.retain(|s| s.space_id != *closed_id);
                // Only quit if the active space was closed and there are no spaces left.
                if self.spaces.is_empty() {
                    self.should_quit = true;
                } else if self.active_space_idx >= self.spaces.len() {
                    self.active_space_idx = self.spaces.len().saturating_sub(1);
                }
                self.needs_redraw = true;
            }
            ServerEvent::AgentCreated(info) => {
                let duration_s = info.detail.as_ref().map(|d| d.duration_s).unwrap_or(0);
                self.agent_start_times.insert(
                    info.id,
                    Instant::now() - std::time::Duration::from_secs(duration_s as u64),
                );
                if info.status == AgentStatus::Blocked {
                    self.agent_blocked_times
                        .entry(info.id)
                        .or_insert_with(Instant::now);
                }
                self.agents.push(info.clone());
                self.sort_agents();
                // Auto-open in Sidebar mode when a new agent is detected, unless already Modal.
                if self.agent_fleet_enabled && self.agent_panel_mode == AgentPanelMode::Hidden {
                    self.agent_panel_mode = AgentPanelMode::Sidebar;
                }
                self.needs_redraw = true;
            }
            ServerEvent::AgentRemoved(id) => {
                // Dismiss Eclipse modal if it belongs to this agent.
                if self.eclipse_modal.as_ref().map(|m| m.agent_id) == Some(*id) {
                    self.eclipse_modal = None;
                }
                // Dismiss Agent Detail modal if it belongs to this agent.
                if let Some(m) = &self.agent_detail_modal {
                    if m.agent_id == *id {
                        self.agent_detail_modal = None;
                    }
                }
                self.agent_start_times.remove(id);
                self.agent_blocked_times.remove(id);
                self.agent_metrics.remove(id);
                self.agents.retain(|a| a.id != *id);
                if let Some(AgentHover::CardBtn { card_idx, .. }) = &self.agent_hovered {
                    if *card_idx >= self.agents.len() {
                        self.agent_hovered = None;
                    }
                }
                self.agent_scroll_offset = self
                    .agent_scroll_offset
                    .min(self.agents.len().saturating_sub(1));
                // Clamp AgentPanel keyboard selection to valid range.
                if let InputMode::AgentPanel { selected } = &mut self.mode {
                    let max = self.agents.len().saturating_sub(1);
                    *selected = (*selected).min(max);
                }
                self.needs_redraw = true;
            }
            ServerEvent::AgentStatusChanged {
                agent_id,
                new_status,
                detail,
            } => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == *agent_id) {
                    agent.status = new_status.clone();
                    agent.detail = detail.clone();
                }
                if new_status == &AgentStatus::Blocked {
                    self.agent_blocked_times
                        .entry(*agent_id)
                        .or_insert_with(Instant::now);
                } else {
                    self.agent_blocked_times.remove(agent_id);
                }
                self.sort_agents();
                self.needs_redraw = true;
            }
            ServerEvent::AgentMetricsUpdated { agent_id, metrics } => {
                self.agent_metrics.insert(*agent_id, metrics.clone());
                if let Some(m) = &mut self.agent_detail_modal {
                    if m.agent_id == *agent_id {
                        m.recent_output_lines = metrics.recent_lines.clone();
                    }
                }
                self.needs_redraw = true;
            }
            ServerEvent::AgentAcpUpdated { agent_id, data } => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.id == *agent_id) {
                    if let Some(detail) = agent.detail.as_mut() {
                        detail.acp = Some(data.clone());
                    }
                }
                // Update the open detail modal in-place if it is showing this agent.
                if let Some(m) = &mut self.agent_detail_modal {
                    if m.agent_id == *agent_id {
                        m.tokens_in = data.tokens_in;
                        m.tokens_out = data.tokens_out;
                        m.current_tool = data.current_tool.clone();
                        m.recent_tools = data.recent_tools.clone();
                        m.total_tool_calls = data.total_tool_calls;
                        m.files_touched = data.files_touched.clone();
                    }
                }
                self.needs_redraw = true;
            }
            ServerEvent::PayloadReady { path } => {
                self.pending_payload_path = Some(path.clone());
                self.needs_redraw = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use orbt_protocol::{
        AgentDetail, AgentInfo, AgentStatus, CellGrid, FullState, PaneInfo, PaneLayout, SpaceInfo,
        TabInfo,
    };

    /// Build a minimal App with one heuristic Working agent for widget smoke tests.
    pub fn make_test_app(w: u16, h: u16) -> App {
        let mut state = minimal_state();
        state.agents.push(AgentInfo {
            id: AgentId(1),
            name: "test-agent".to_string(),
            space_id: SpaceId(1),
            model: "claude".to_string(),
            status: AgentStatus::Working,
            pane_id: Some(PaneId(1)),
            detail: Some(AgentDetail {
                task: Some("Test task".to_string()),
                block_msg: None,
                progress: Some(0.4),
                duration_s: 60,
                acp: None,
            }),
            protocol: orbt_protocol::AgentProtocol::Heuristic,
            launch_cmd: None,
        });
        App::from_welcome(&state, w, h)
    }

    /// Helper: build a minimal FullState for constructing App instances in tests.
    fn minimal_state() -> FullState {
        FullState {
            spaces: vec![SpaceInfo {
                id: SpaceId(1),
                name: "test".to_string(),
                path: "/tmp/test".to_string(),
                tabs: vec![TabInfo {
                    id: TabId(1),
                    name: "dev".to_string(),
                    layout: PaneLayout::Leaf(PaneId(1)),
                    active_pane: PaneId(1),
                }],
                active_tab: TabId(1),
                panes: vec![PaneInfo {
                    id: PaneId(1),
                    tab_id: TabId(1),
                    title: String::new(),
                    cwd: "/tmp".to_string(),
                    cell_grid: CellGrid::new(80, 24),
                }],
            }],
            active_space: SpaceId(1),
            agents: vec![],
        }
    }

    fn make_agent(id: u32, status: AgentStatus) -> AgentInfo {
        AgentInfo {
            id: AgentId(id),
            name: format!("agent-{id}"),
            space_id: SpaceId(1),
            model: "claude".to_string(),
            status,
            pane_id: Some(PaneId(1)),
            detail: Some(AgentDetail {
                task: None,
                block_msg: None,
                progress: None,
                duration_s: 0,
                acp: None,
            }),
            protocol: orbt_protocol::AgentProtocol::Heuristic,
            launch_cmd: None,
        }
    }

    #[test]
    fn sync_from_server_different_dimensions() {
        let mut pane = PaneState::new(80, 24);
        let server_grid = CellGrid {
            cols: 120,
            rows: 30,
            cells: vec![orbt_protocol::Cell::default(); 120 * 30],
            cursor_x: 10,
            cursor_y: 5,
            cursor_visible: true,
            mouse_reporting: false,
            mouse_sgr: false,
        };
        pane.sync_from_server(&server_grid);
        assert_eq!(pane.parser.grid.cols, 120);
        assert_eq!(pane.parser.grid.rows, 30);
        assert_eq!(pane.parser.grid.cells.len(), 120 * 30);
        assert_eq!(pane.parser.grid.cursor_x, 10);
        assert_eq!(pane.parser.grid.cursor_y, 5);
    }

    #[test]
    fn from_welcome_builds_correct_state() {
        let state = minimal_state();
        let app = App::from_welcome(&state, 80, 24);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].name, "dev");
        assert_eq!(app.active_pane, PaneId(1));
        assert_eq!(app.active_tab_id, TabId(1));
        assert_eq!(app.space_name, "test");
        assert!(app.panes.contains_key(&PaneId(1)));
        assert_eq!(app.mode, InputMode::Normal);
        assert!(!app.should_quit);
    }

    #[test]
    fn sort_agents_priority_order() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        app.agents = vec![
            make_agent(1, AgentStatus::Done),
            make_agent(2, AgentStatus::Working),
            make_agent(3, AgentStatus::Blocked),
            make_agent(4, AgentStatus::Error),
            make_agent(5, AgentStatus::Idle),
        ];
        app.sort_agents();
        let order: Vec<AgentId> = app.agents.iter().map(|a| a.id).collect();
        // Blocked(0) < Working(1) < Error(2) < Idle(3) < Done(4)
        assert_eq!(
            order,
            vec![AgentId(3), AgentId(2), AgentId(4), AgentId(5), AgentId(1)]
        );
    }

    #[test]
    fn sort_agents_preserves_keyboard_selection() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        app.agents = vec![
            make_agent(1, AgentStatus::Idle),
            make_agent(2, AgentStatus::Working),
            make_agent(3, AgentStatus::Done),
        ];
        // Select agent 2 (idx=1 before sort)
        app.mode = InputMode::AgentPanel { selected: 1 };
        app.sort_agents();
        // After sort, Working(agent 2) should be at idx=0
        if let InputMode::AgentPanel { selected } = app.mode {
            assert_eq!(app.agents[selected].id, AgentId(2));
        } else {
            panic!("Expected AgentPanel mode");
        }
    }

    #[test]
    fn has_active_agents_detection() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        assert!(!app.has_active_agents());

        app.agents.push(make_agent(1, AgentStatus::Idle));
        assert!(!app.has_active_agents());

        app.agents.push(make_agent(2, AgentStatus::Working));
        assert!(app.has_active_agents());
    }

    #[test]
    fn next_prev_tab_cycles() {
        let mut state = minimal_state();
        state.spaces[0].tabs.push(TabInfo {
            id: TabId(2),
            name: "build".to_string(),
            layout: PaneLayout::Leaf(PaneId(2)),
            active_pane: PaneId(2),
        });
        state.spaces[0].panes.push(PaneInfo {
            id: PaneId(2),
            tab_id: TabId(2),
            title: String::new(),
            cwd: "/tmp".to_string(),
            cell_grid: CellGrid::new(80, 24),
        });
        let mut app = App::from_welcome(&state, 80, 24);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.active_tab_id, TabId(1));

        app.next_tab();
        assert_eq!(app.active_tab, 1);
        assert_eq!(app.active_tab_id, TabId(2));

        app.next_tab();
        assert_eq!(app.active_tab, 0); // wraps around

        app.prev_tab();
        assert_eq!(app.active_tab, 1); // wraps backward
    }

    #[test]
    fn cycle_focus_rotates_panes() {
        let mut state = minimal_state();
        state.spaces[0].tabs[0].layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
        };
        state.spaces[0].panes.push(PaneInfo {
            id: PaneId(2),
            tab_id: TabId(1),
            title: String::new(),
            cwd: "/tmp".to_string(),
            cell_grid: CellGrid::new(80, 24),
        });
        let mut app = App::from_welcome(&state, 80, 24);
        app.active_pane = PaneId(1);

        app.cycle_focus();
        assert_eq!(app.active_pane, PaneId(2));

        app.cycle_focus();
        assert_eq!(app.active_pane, PaneId(1));
    }

    #[test]
    fn open_close_context_menu() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        assert!(app.context_menu.is_none());

        app.open_context_menu(10, 5, ContextMenuTarget::Pane(PaneId(1)));
        assert!(app.context_menu.is_some());
        let menu = app.context_menu.as_ref().unwrap();
        assert_eq!(menu.x, 10);
        assert_eq!(menu.y, 5);
        assert_eq!(menu.selected, 0);
        // Pane context menu has at least split + close items
        assert!(menu.items.len() >= 4);

        app.close_context_menu();
        assert!(app.context_menu.is_none());
    }

    #[test]
    fn context_menu_tab_items() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        app.open_context_menu(0, 0, ContextMenuTarget::Tab(TabId(1)));
        let menu = app.context_menu.as_ref().unwrap();
        let action_ids: Vec<&str> = menu
            .items
            .iter()
            .filter_map(|i| match i {
                ContextMenuItem::Action { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        assert!(action_ids.contains(&"new_tab"));
        assert!(action_ids.contains(&"close_tab"));
    }

    #[test]
    fn handle_agent_created_event() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        app.agent_fleet_enabled = true;
        assert!(app.agents.is_empty());

        let info = make_agent(10, AgentStatus::Working);
        app.handle_server_event(&ServerEvent::AgentCreated(info));
        assert_eq!(app.agents.len(), 1);
        assert_eq!(app.agents[0].id, AgentId(10));
        assert_eq!(app.agent_panel_mode, AgentPanelMode::Sidebar); // auto-opens
        assert!(app.agent_start_times.contains_key(&AgentId(10)));
    }

    #[test]
    fn handle_agent_removed_cleans_state() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        app.agents.push(make_agent(5, AgentStatus::Working));
        app.agent_start_times.insert(AgentId(5), Instant::now());
        app.agent_blocked_times.insert(AgentId(5), Instant::now());

        app.handle_server_event(&ServerEvent::AgentRemoved(AgentId(5)));
        assert!(app.agents.is_empty());
        assert!(!app.agent_start_times.contains_key(&AgentId(5)));
        assert!(!app.agent_blocked_times.contains_key(&AgentId(5)));
    }

    #[test]
    fn handle_agent_removed_dismisses_eclipse_modal() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        app.agents.push(make_agent(7, AgentStatus::Blocked));
        app.eclipse_modal = Some(EclipseModalState {
            agent_id: AgentId(7),
            agent_name: "agent-7".to_string(),
            block_msg: "needs input".to_string(),
            response: String::new(),
            model: "claude".to_string(),
            task: None,
            progress: None,
            cwd: None,
            blocked_duration_s: 0,
        });

        app.handle_server_event(&ServerEvent::AgentRemoved(AgentId(7)));
        assert!(app.eclipse_modal.is_none());
    }

    #[test]
    fn handle_agent_status_changed_tracks_blocked_time() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        app.agents.push(make_agent(3, AgentStatus::Working));

        // Transition to Blocked
        app.handle_server_event(&ServerEvent::AgentStatusChanged {
            agent_id: AgentId(3),
            new_status: AgentStatus::Blocked,
            detail: None,
        });
        assert!(app.agent_blocked_times.contains_key(&AgentId(3)));
        assert_eq!(app.agents[0].status, AgentStatus::Blocked);

        // Transition back to Working
        app.handle_server_event(&ServerEvent::AgentStatusChanged {
            agent_id: AgentId(3),
            new_status: AgentStatus::Working,
            detail: None,
        });
        assert!(!app.agent_blocked_times.contains_key(&AgentId(3)));
    }

    #[test]
    fn handle_agent_removed_clamps_panel_selection() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        app.agents = vec![
            make_agent(1, AgentStatus::Working),
            make_agent(2, AgentStatus::Working),
            make_agent(3, AgentStatus::Working),
        ];
        app.mode = InputMode::AgentPanel { selected: 2 };

        app.handle_server_event(&ServerEvent::AgentRemoved(AgentId(3)));
        if let InputMode::AgentPanel { selected } = app.mode {
            assert!(selected < app.agents.len());
        } else {
            panic!("Expected AgentPanel mode");
        }
    }

    #[test]
    fn handle_space_created_event() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 80, 24);
        assert_eq!(app.spaces.len(), 1);

        let new_space = SpaceInfo {
            id: SpaceId(2),
            name: "build".to_string(),
            path: "/tmp/build".to_string(),
            tabs: vec![TabInfo {
                id: TabId(10),
                name: "main".to_string(),
                layout: PaneLayout::Leaf(PaneId(10)),
                active_pane: PaneId(10),
            }],
            active_tab: TabId(10),
            panes: vec![PaneInfo {
                id: PaneId(10),
                tab_id: TabId(10),
                title: String::new(),
                cwd: "/tmp/build".to_string(),
                cell_grid: CellGrid::new(80, 24),
            }],
        };
        app.handle_server_event(&ServerEvent::SpaceCreated(new_space));
        assert_eq!(app.spaces.len(), 2);
        assert_eq!(app.active_space_idx, 1); // auto-selects new space
        assert_eq!(app.spaces[1].name, "build");
    }

    #[test]
    fn pane_in_current_tab_check() {
        let mut state = minimal_state();
        state.spaces[0].tabs[0].layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
        };
        state.spaces[0].panes.push(PaneInfo {
            id: PaneId(2),
            tab_id: TabId(1),
            title: String::new(),
            cwd: "/tmp".to_string(),
            cell_grid: CellGrid::new(80, 24),
        });
        let app = App::from_welcome(&state, 80, 24);
        assert!(app.pane_in_current_tab(PaneId(1)));
        assert!(app.pane_in_current_tab(PaneId(2)));
        assert!(!app.pane_in_current_tab(PaneId(99)));
    }

    #[test]
    fn settings_persistence_roundtrip() {
        let settings = UserSettings {
            theme: "orange".to_string(),
            sidebar_visible: false,
            agent_panel_mode: AgentPanelMode::Modal,
            agent_panel_visible: false,
            agent_fleet_enabled: false,
        };
        let toml_str = toml::to_string(&settings).unwrap();
        let restored: UserSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.theme, "orange");
        assert!(!restored.sidebar_visible);
        assert_eq!(restored.agent_panel_mode, AgentPanelMode::Modal);
    }

    #[test]
    fn settings_legacy_migration() {
        // Old settings.toml with agent_panel_visible=true and no agent_panel_mode.
        let toml_str = "theme = \"orbt\"\nsidebar_visible = true\nagent_panel_visible = true\n";
        let mut s: UserSettings = toml::from_str(toml_str).unwrap();
        if s.agent_panel_mode == AgentPanelMode::Hidden && s.agent_panel_visible {
            s.agent_panel_mode = AgentPanelMode::Sidebar;
        }
        assert_eq!(s.agent_panel_mode, AgentPanelMode::Sidebar);
    }

    #[test]
    fn default_settings_values() {
        let settings = UserSettings::default();
        assert_eq!(settings.theme, "orbt");
        assert!(settings.sidebar_visible);
        assert_eq!(settings.agent_panel_mode, AgentPanelMode::Hidden);
    }
}
