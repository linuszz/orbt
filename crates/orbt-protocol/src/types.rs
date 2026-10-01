//! Shared types carried by the IPC protocol. See `06_tech-design/03-ipc-protocol.md` §3
//! and `06_tech-design/05-vt-emulation.md` §3 for `Cell`/`CellFlags`/`TermColor` size analysis.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub u32);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpaceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImageId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDir {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentStatus {
    #[default]
    Idle,
    Working,
    Blocked,
    Error,
    Done,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentDetail {
    pub task: Option<String>,
    pub block_msg: Option<String>,
    pub progress: Option<f32>, // generic progress bar (keep; separate from context_percent)
    pub duration_s: u32,
    /// Populated only for ACP-connected agents.
    #[serde(default)]
    pub acp: Option<AcpDetail>,
    /// Context window usage as a fraction 0.0–1.0.
    #[serde(default)]
    pub context_percent: Option<f32>,
    /// Number of /compact operations performed in this session.
    #[serde(default)]
    pub compaction_count: u32,
    /// Agent CLI identifier: "claude" | "codex" | "copilot" | etc.
    #[serde(default)]
    pub agent_cli: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolCallStatus {
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: u32,
    /// Tool name as reported by the agent, e.g. "Bash", "Read", "Write".
    pub tool: String,
    /// Args truncated to ≤60 chars by the daemon.
    pub args_summary: String,
    pub status: ToolCallStatus,
    /// Wall-clock duration in ms; None while Running.
    pub duration_ms: Option<u32>,
    /// Complete tool output, capped to 8 KiB on the daemon side.
    /// None when not yet captured or not available.
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileKind {
    Read,
    Modified,
    Created,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTouched {
    pub path: String,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubAgentInfo {
    pub name: String,
    pub status: AgentStatus,
    pub tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcpDetail {
    /// Tool call currently executing, if any.
    pub current_tool: Option<ToolCall>,
    /// Last ≤20 completed calls, newest-first.
    pub recent_tools: Vec<ToolCall>,
    /// Running total including evicted calls.
    pub total_tool_calls: u32,
    pub tokens_in: u32,
    pub tokens_out: u32,
    /// Deduplicated, newest-first, capped at 20.
    pub files_touched: Vec<FileTouched>,
    /// Conversation turns (assistant reply count).
    #[serde(default)]
    pub turn_count: u32,
    /// Agent session identifier, truncated to 8 chars for display.
    #[serde(default)]
    pub session_id: String,
    /// Agent CLI version string.
    #[serde(default)]
    pub agent_version: String,
    /// Per-turn input token counts for context sparkline; newest-last, capped at 20.
    #[serde(default)]
    pub context_history: Vec<u32>,
    /// Active sub-agents spawned by this agent.
    #[serde(default)]
    pub subagents: Vec<SubAgentInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMetrics {
    pub cpu_percent: Option<f32>,
    pub rss_kb: Option<u32>,
    pub recent_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CellFlags(pub u8);

impl CellFlags {
    pub const BOLD: u8 = 0b0001;
    pub const ITALIC: u8 = 0b0010;
    pub const UNDERLINE: u8 = 0b0100;
    pub const DIM: u8 = 0b1000;
    pub const REVERSE: u8 = 0b0001_0000;

    pub fn bold(self) -> bool {
        self.0 & Self::BOLD != 0
    }
    pub fn italic(self) -> bool {
        self.0 & Self::ITALIC != 0
    }
    pub fn underline(self) -> bool {
        self.0 & Self::UNDERLINE != 0
    }
    pub fn dim(self) -> bool {
        self.0 & Self::DIM != 0
    }
    pub fn reverse(self) -> bool {
        self.0 & Self::REVERSE != 0
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum TermColor {
    #[default]
    Default,
    Ansi(u8),
    Ansi256(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: TermColor,
    pub bg: TermColor,
    pub flags: CellFlags,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellGrid {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<Cell>,
    pub cursor_x: u16,
    pub cursor_y: u16,
    #[serde(default = "default_true")]
    pub cursor_visible: bool,
    #[serde(default)]
    pub mouse_reporting: bool,
    #[serde(default)]
    pub mouse_sgr: bool,
}

impl CellGrid {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); cols as usize * rows as usize],
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
            mouse_reporting: false,
            mouse_sgr: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FullState {
    pub spaces: Vec<SpaceInfo>,
    pub active_space: SpaceId,
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceInfo {
    pub id: SpaceId,
    pub name: String,
    pub path: String,
    pub tabs: Vec<TabInfo>,
    pub active_tab: TabId,
    pub panes: Vec<PaneInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaneLayout {
    Leaf(PaneId),
    Split {
        direction: SplitDir,
        first: Box<PaneLayout>,
        second: Box<PaneLayout>,
        #[serde(default = "default_ratio")]
        ratio: f32,
    },
    /// Scrollable strip: columns laid out left to right on an unbounded band.
    /// Panes stack vertically inside a column, so opening one sideways adds a
    /// column while opening one downwards joins the current column. Neither
    /// resizes the columns that already exist; the viewport scrolls to keep
    /// the focused pane visible.
    Strip {
        columns: Vec<StripColumn>,
        #[serde(default = "default_strip_column_width")]
        column_width: u16,
    },
}

/// One column of a scrollable strip. Panes within it share the column's height.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripColumn {
    pub panes: Vec<PaneId>,
}

impl StripColumn {
    pub fn single(pane: PaneId) -> Self {
        Self { panes: vec![pane] }
    }
}

fn default_ratio() -> f32 {
    0.5
}

fn default_strip_column_width() -> u16 {
    80
}

/// Which layout a newly created tab starts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TabLayout {
    #[default]
    Bsp,
    Strip,
}

impl PaneLayout {
    pub fn split_leaf(&mut self, target: PaneId, _direction: SplitDir, new_id: PaneId) -> bool {
        match self {
            PaneLayout::Leaf(id) if *id == target => {
                *self = PaneLayout::Split {
                    direction: _direction,
                    first: Box::new(PaneLayout::Leaf(target)),
                    second: Box::new(PaneLayout::Leaf(new_id)),
                    ratio: 0.5,
                };
                true
            }
            PaneLayout::Leaf(_) => false,
            PaneLayout::Split { first, second, .. } => {
                first.split_leaf(target, _direction, new_id)
                    || second.split_leaf(target, _direction, new_id)
            }
            PaneLayout::Strip { columns, .. } => {
                let Some(col) = columns.iter_mut().find(|c| c.panes.contains(&target)) else {
                    return false;
                };
                match _direction {
                    SplitDir::Horizontal => {
                        let at = columns
                            .iter()
                            .position(|c| c.panes.contains(&target))
                            .unwrap_or(0);
                        columns.insert(at + 1, StripColumn::single(new_id));
                    }
                    SplitDir::Vertical => col.panes.push(new_id),
                }
                true
            }
        }
    }

    pub fn set_split_ratio(&mut self, first_pane: PaneId, second_pane: PaneId, ratio: f32) -> bool {
        let ratio = if ratio.is_finite() {
            ratio.clamp(0.1, 0.9)
        } else {
            0.5
        };
        match self {
            PaneLayout::Leaf(_) => false,
            PaneLayout::Split {
                first,
                second,
                ratio: r,
                ..
            } => {
                let first_leaf = first.leaves().first().copied();
                let second_leaf = second.leaves().first().copied();
                if first_leaf == Some(first_pane) && second_leaf == Some(second_pane) {
                    *r = ratio;
                    return true;
                }
                first.set_split_ratio(first_pane, second_pane, ratio)
                    || second.set_split_ratio(first_pane, second_pane, ratio)
            }
            PaneLayout::Strip { .. } => {
                // Strips resize by absolute width via SetColumnWidth; a split
                // ratio carries no meaning here.
                false
            }
        }
    }

    pub fn remove_leaf(&mut self, target: PaneId) -> bool {
        match self {
            PaneLayout::Leaf(id) => *id != target,
            PaneLayout::Split { first, second, .. } => {
                if let PaneLayout::Leaf(id) = **first {
                    if id == target {
                        *self = (**second).clone();
                        return true;
                    }
                }
                if let PaneLayout::Leaf(id) = **second {
                    if id == target {
                        *self = (**first).clone();
                        return true;
                    }
                }
                first.remove_leaf(target);
                second.remove_leaf(target);
                true
            }
            PaneLayout::Strip { columns, .. } => {
                for col in columns.iter_mut() {
                    if let Some(idx) = col.panes.iter().position(|&p| p == target) {
                        col.panes.remove(idx);
                    }
                }
                columns.retain(|c| !c.panes.is_empty());
                if let [only] = &columns[..] {
                    if let [pane] = &only.panes[..] {
                        *self = PaneLayout::Leaf(*pane);
                    }
                }
                true
            }
        }
    }

    pub fn is_strip(&self) -> bool {
        matches!(self, PaneLayout::Strip { .. })
    }

    pub fn leaves(&self) -> Vec<PaneId> {
        match self {
            PaneLayout::Leaf(id) => vec![*id],
            PaneLayout::Split { first, second, .. } => {
                let mut v = first.leaves();
                v.extend(second.leaves());
                v
            }
            PaneLayout::Strip { columns, .. } => columns
                .iter()
                .flat_map(|c| c.panes.iter().copied())
                .collect(),
        }
    }

    /// Rebuild this layout in `target`, leaving every pane where the user can
    /// still see it.
    ///
    /// A strip gives every pane a column of its own, so a tree collapses to one
    /// pane per column in the order they were already arranged. Going the other
    /// way, each column becomes a vertical chain and the chains are joined side
    /// by side, which puts the panes back into the arrangement just described.
    /// Asking for the layout a tab is already in changes nothing.
    pub fn converted(&self, target: TabLayout) -> PaneLayout {
        match (self, target) {
            (PaneLayout::Strip { .. }, TabLayout::Strip)
            | (PaneLayout::Split { .. }, TabLayout::Bsp)
            | (PaneLayout::Leaf(_), TabLayout::Bsp) => self.clone(),
            (_, TabLayout::Strip) => {
                let columns = self.leaves().into_iter().map(StripColumn::single).collect();
                PaneLayout::Strip {
                    columns,
                    column_width: default_strip_column_width(),
                }
            }
            (PaneLayout::Strip { columns, .. }, TabLayout::Bsp) => {
                let chains: Vec<PaneLayout> = columns
                    .iter()
                    .filter(|c| !c.panes.is_empty())
                    .map(|c| Self::chain(&c.panes, SplitDir::Vertical))
                    .collect();
                Self::join(&chains)
            }
        }
    }

    /// Fold `panes` into a balanced tree along `direction`, halving at each
    /// level so every pane ends up with the same share of the space.
    fn chain(panes: &[PaneId], direction: SplitDir) -> PaneLayout {
        match panes {
            [] => PaneLayout::Leaf(PaneId(0)),
            [only] => PaneLayout::Leaf(*only),
            [..] => {
                let mid = panes.len() / 2;
                PaneLayout::Split {
                    direction,
                    first: Box::new(Self::chain(&panes[..mid], direction)),
                    second: Box::new(Self::chain(&panes[mid..], direction)),
                    ratio: mid as f32 / panes.len() as f32,
                }
            }
        }
    }

    /// Join already-built trees into one, splitting them off sideways.
    fn join(nodes: &[PaneLayout]) -> PaneLayout {
        match nodes {
            [] => PaneLayout::Leaf(PaneId(0)),
            [only] => only.clone(),
            [..] => {
                let mid = nodes.len() / 2;
                PaneLayout::Split {
                    direction: SplitDir::Horizontal,
                    first: Box::new(Self::join(&nodes[..mid])),
                    second: Box::new(Self::join(&nodes[mid..])),
                    ratio: mid as f32 / nodes.len() as f32,
                }
            }
        }
    }

    pub fn find_pane_in_direction(
        &self,
        current: PaneId,
        split_dir: SplitDir,
        positive: bool,
    ) -> Option<PaneId> {
        match self {
            PaneLayout::Leaf(_) => None,
            PaneLayout::Split {
                direction,
                first,
                second,
                ..
            } => {
                let in_first = first.leaves().contains(&current);
                let in_second = second.leaves().contains(&current);

                if *direction == split_dir && ((positive && in_first) || (!positive && in_second)) {
                    let target = if positive { second } else { first };
                    target.leaves().first().copied()
                } else {
                    first
                        .find_pane_in_direction(current, split_dir, positive)
                        .or_else(|| second.find_pane_in_direction(current, split_dir, positive))
                }
            }
            PaneLayout::Strip { columns, .. } => {
                let (col_idx, row_idx) = columns.iter().enumerate().find_map(|(ci, c)| {
                    c.panes
                        .iter()
                        .position(|&p| p == current)
                        .map(|ri| (ci, ri))
                })?;
                match split_dir {
                    SplitDir::Vertical => {
                        let col = &columns[col_idx];
                        let next = if positive {
                            row_idx + 1
                        } else {
                            row_idx.checked_sub(1)?
                        };
                        col.panes.get(next).copied()
                    }
                    SplitDir::Horizontal => {
                        // Step to the neighbouring column, keeping the same row
                        // when it exists there.
                        let next_col = if positive {
                            col_idx + 1
                        } else {
                            col_idx.checked_sub(1)?
                        };
                        let target = columns.get(next_col)?;
                        target
                            .panes
                            .get(row_idx)
                            .or_else(|| target.panes.last())
                            .copied()
                    }
                }
            }
        }
    }

    pub fn as_strip(&self) -> Option<(&[StripColumn], u16)> {
        match self {
            PaneLayout::Strip {
                columns,
                column_width,
            } => Some((columns, *column_width)),
            _ => None,
        }
    }

    pub fn set_column_width(&mut self, width: u16) -> bool {
        match self {
            PaneLayout::Strip { column_width, .. } => {
                *column_width = width.clamp(20, 400);
                true
            }
            _ => false,
        }
    }

    /// Swap `pane` with its neighbour in a strip. Returns false on split trees
    /// and at the ends of the strip, where there is nothing to swap with.
    pub fn swap_pane(&mut self, pane: PaneId, towards_left: bool) -> bool {
        let PaneLayout::Strip { columns, .. } = self else {
            return false;
        };
        let Some((col_idx, row_idx)) = columns
            .iter()
            .enumerate()
            .find_map(|(ci, c)| c.panes.iter().position(|&p| p == pane).map(|ri| (ci, ri)))
        else {
            return false;
        };
        // Left/right reorders columns; the command only ever moves a pane
        // sideways, so that is the axis it acts on.
        let step: isize = if towards_left { -1 } else { 1 };
        let target = col_idx as isize + step;
        if target < 0 || target as usize >= columns.len() {
            return false;
        }
        columns.swap(col_idx, target as usize);
        let _ = row_idx;
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneInfo {
    pub id: PaneId,
    pub tab_id: TabId,
    pub title: String,
    pub cwd: String,
    pub cell_grid: CellGrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: TabId,
    pub name: String,
    pub layout: PaneLayout,
    pub active_pane: PaneId,
}

/// Which protocol is used to track this agent's status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentProtocol {
    /// Detected via PTY output heuristics and /proc polling.
    #[default]
    Heuristic,
    /// Agent binary supports ACP but is running in interactive PTY mode (not ACP-connected).
    AcpCapable,
    /// Actively connected to the agent via ACP JSON-RPC.
    AcpConnected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: AgentId,
    pub name: String,
    pub space_id: SpaceId,
    pub pane_id: Option<PaneId>,
    pub model: String,
    pub status: AgentStatus,
    pub detail: Option<AgentDetail>,
    /// Detection/connection protocol. Defaults to Heuristic for backward compatibility.
    #[serde(default)]
    pub protocol: AgentProtocol,
    /// Command that launched this agent (if via AgentLaunch); used for restart re-exec.
    #[serde(default)]
    pub launch_cmd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLaunchRequest {
    pub name: String,
    pub model: String,
    pub cwd: String,
    pub space_id: SpaceId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollbackLine {
    pub cells: Vec<Cell>,
    pub width: u16,
    pub seq: u64,
}

#[test]
fn strip_swap_moves_pane_between_slots() {
    let mut layout = PaneLayout::Strip {
        columns: vec![
            StripColumn::single(PaneId(1)),
            StripColumn::single(PaneId(2)),
            StripColumn::single(PaneId(3)),
        ],
        column_width: 80,
    };
    assert!(layout.swap_pane(PaneId(2), true));
    assert_eq!(layout.leaves(), vec![PaneId(2), PaneId(1), PaneId(3)]);

    assert!(layout.swap_pane(PaneId(1), false));
    assert_eq!(layout.leaves(), vec![PaneId(2), PaneId(3), PaneId(1)]);

    // Ends of the strip have no neighbour.
    assert!(!layout.swap_pane(PaneId(2), true));
    assert!(!layout.swap_pane(PaneId(1), false));
    // Unknown pane is a no-op, not a panic.
    assert!(!layout.swap_pane(PaneId(99), true));
}

#[test]
fn strip_column_width_clamps_and_ignores_split_trees() {
    let mut strip = PaneLayout::Strip {
        columns: vec![StripColumn::single(PaneId(1))],
        column_width: 80,
    };
    assert!(strip.set_column_width(120));
    assert_eq!(strip.as_strip().map(|(_, w)| w), Some(120));
    assert!(strip.set_column_width(5));
    assert_eq!(
        strip.as_strip().map(|(_, w)| w),
        Some(20),
        "clamped up to the minimum"
    );
    assert!(strip.set_column_width(9999));
    assert_eq!(
        strip.as_strip().map(|(_, w)| w),
        Some(400),
        "clamped down to the maximum"
    );

    let mut tree = PaneLayout::Split {
        direction: SplitDir::Horizontal,
        first: Box::new(PaneLayout::Leaf(PaneId(1))),
        second: Box::new(PaneLayout::Leaf(PaneId(2))),
        ratio: 0.5,
    };
    assert!(!tree.set_column_width(100));
    assert!(!tree.swap_pane(PaneId(1), true));
}

#[test]
fn strip_ratio_resize_is_a_no_op() {
    // Strips size by absolute width; ResizeSplit must not quietly scale
    // the column by the drag ratio.
    let mut strip = PaneLayout::Strip {
        columns: vec![
            StripColumn::single(PaneId(1)),
            StripColumn::single(PaneId(2)),
        ],
        column_width: 80,
    };
    assert!(!strip.set_split_ratio(PaneId(1), PaneId(2), 0.9));
    assert_eq!(strip.as_strip().map(|(_, w)| w), Some(80));
}

#[test]
fn strip_split_h_adds_a_column_and_split_v_stacks() {
    let mut layout = PaneLayout::Strip {
        columns: vec![StripColumn::single(PaneId(1))],
        column_width: 80,
    };
    // Sideways opens a new column to the right; the existing column is untouched.
    assert!(layout.split_leaf(PaneId(1), SplitDir::Horizontal, PaneId(2)));
    assert_eq!(layout.leaves(), vec![PaneId(1), PaneId(2)]);
    assert_eq!(layout.as_strip().unwrap().0.len(), 2, "two columns now");

    // Downwards joins the pane's own column instead of creating one.
    assert!(layout.split_leaf(PaneId(1), SplitDir::Vertical, PaneId(3)));
    let columns = layout.as_strip().unwrap().0;
    assert_eq!(columns.len(), 2, "still two columns");
    assert_eq!(columns[0].panes, vec![PaneId(1), PaneId(3)], "stacked");
    assert_eq!(columns[1].panes, vec![PaneId(2)]);
}

#[test]
fn strip_vertical_navigation_walks_within_a_column() {
    let layout = PaneLayout::Strip {
        columns: vec![
            StripColumn {
                panes: vec![PaneId(1), PaneId(2)],
            },
            StripColumn::single(PaneId(3)),
        ],
        column_width: 80,
    };
    assert_eq!(
        layout.find_pane_in_direction(PaneId(1), SplitDir::Vertical, true),
        Some(PaneId(2)),
        "down moves to the pane below"
    );
    assert_eq!(
        layout.find_pane_in_direction(PaneId(2), SplitDir::Vertical, false),
        Some(PaneId(1))
    );
    assert_eq!(
        layout.find_pane_in_direction(PaneId(2), SplitDir::Vertical, true),
        None,
        "nothing below the last pane in the column"
    );
}

#[test]
fn strip_horizontal_navigation_crosses_columns() {
    let layout = PaneLayout::Strip {
        columns: vec![
            StripColumn {
                panes: vec![PaneId(1), PaneId(2)],
            },
            StripColumn::single(PaneId(3)),
        ],
        column_width: 80,
    };
    assert_eq!(
        layout.find_pane_in_direction(PaneId(1), SplitDir::Horizontal, true),
        Some(PaneId(3)),
        "row 1 has no counterpart, so it falls back to the column's only pane"
    );
    assert_eq!(
        layout.find_pane_in_direction(PaneId(3), SplitDir::Horizontal, false),
        Some(PaneId(1)),
        "moving back lands on the same row when it exists"
    );
}

#[test]
fn strip_removing_the_last_pane_of_a_column_drops_it() {
    let mut layout = PaneLayout::Strip {
        columns: vec![
            StripColumn {
                panes: vec![PaneId(1), PaneId(2)],
            },
            StripColumn::single(PaneId(3)),
        ],
        column_width: 80,
    };
    layout.remove_leaf(PaneId(2));
    let columns = layout.as_strip().unwrap().0;
    assert_eq!(columns.len(), 2, "the column survives while a pane remains");
    assert_eq!(columns[0].panes, vec![PaneId(1)]);

    // Removing it empties the first column and drops it, leaving one pane
    // overall, which collapses back to a plain Leaf.
    layout.remove_leaf(PaneId(1));
    assert!(
        matches!(layout, PaneLayout::Leaf(_)),
        "one pane is a Leaf again"
    );
    assert_eq!(layout.leaves(), vec![PaneId(3)]);
}

#[test]
fn strip_collapses_to_leaf_when_a_single_pane_remains() {
    let mut layout = PaneLayout::Strip {
        columns: vec![
            StripColumn::single(PaneId(1)),
            StripColumn::single(PaneId(2)),
        ],
        column_width: 80,
    };
    layout.remove_leaf(PaneId(1));
    assert!(matches!(layout, PaneLayout::Leaf(_)), "one pane is a Leaf");
    assert_eq!(layout.leaves(), vec![PaneId(2)]);
}

#[cfg(test)]
/// Every split ratio in a tree, left to right and top to bottom.
fn split_ratios(node: &PaneLayout) -> Vec<f32> {
    match node {
        PaneLayout::Split {
            first,
            second,
            ratio,
            ..
        } => {
            let mut v = vec![*ratio];
            v.extend(split_ratios(first));
            v.extend(split_ratios(second));
            v
        }
        _ => Vec::new(),
    }
}

#[test]
fn converting_a_tree_to_a_strip_gives_every_pane_a_column() {
    let tree = PaneLayout::Split {
        direction: SplitDir::Horizontal,
        first: Box::new(PaneLayout::Split {
            direction: SplitDir::Vertical,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
            ratio: 0.5,
        }),
        second: Box::new(PaneLayout::Leaf(PaneId(3))),
        ratio: 0.5,
    };

    let PaneLayout::Strip { columns, .. } = tree.converted(TabLayout::Strip) else {
        panic!("a tree should convert to a strip");
    };
    assert_eq!(columns.len(), 3, "one column per pane");
    assert_eq!(
        columns.iter().map(|c| c.panes.clone()).collect::<Vec<_>>(),
        vec![vec![PaneId(1)], vec![PaneId(2)], vec![PaneId(3)]],
        "columns keep the tree's visual order"
    );
}

#[test]
fn converting_a_strip_back_to_a_tree_restores_the_columns() {
    let strip = PaneLayout::Strip {
        columns: vec![
            StripColumn {
                panes: vec![PaneId(1), PaneId(2)],
            },
            StripColumn::single(PaneId(3)),
        ],
        column_width: 80,
    };

    let tree = strip.converted(TabLayout::Bsp);
    assert!(matches!(tree, PaneLayout::Split { .. }));
    assert_eq!(tree.leaves(), vec![PaneId(1), PaneId(2), PaneId(3)]);
    assert_eq!(
        split_ratios(&tree),
        vec![0.5, 0.5],
        "each pane gets an equal share, so the tree renders evenly"
    );
}

#[test]
fn converting_to_the_layout_already_in_use_changes_nothing() {
    let tree = PaneLayout::Split {
        direction: SplitDir::Vertical,
        first: Box::new(PaneLayout::Leaf(PaneId(1))),
        second: Box::new(PaneLayout::Leaf(PaneId(2))),
        ratio: 0.7,
    };
    let strip = PaneLayout::Strip {
        columns: vec![
            StripColumn {
                panes: vec![PaneId(1), PaneId(2)],
            },
            StripColumn::single(PaneId(3)),
        ],
        column_width: 42,
    };

    assert_eq!(
        split_ratios(&tree.converted(TabLayout::Bsp)),
        split_ratios(&tree)
    );
    let PaneLayout::Strip {
        columns,
        column_width,
    } = strip.converted(TabLayout::Strip)
    else {
        panic!("stacking must survive asking for a strip again");
    };
    assert_eq!(column_width, 42, "the configured width is left alone");
    assert_eq!(columns[0].panes, vec![PaneId(1), PaneId(2)]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_detail_bincode_roundtrip() {
        use bincode::{
            config::standard,
            serde::{decode_from_slice, encode_to_vec},
        };
        let detail = AgentDetail {
            task: Some("impl auth".into()),
            block_msg: None,
            progress: Some(0.7),
            duration_s: 120,
            acp: Some(AcpDetail {
                current_tool: Some(ToolCall {
                    id: 3,
                    tool: "Bash".into(),
                    args_summary: "cargo test".into(),
                    status: ToolCallStatus::Running,
                    duration_ms: None,
                    output: None,
                }),
                recent_tools: vec![ToolCall {
                    id: 2,
                    tool: "Read".into(),
                    args_summary: "src/lib.rs".into(),
                    status: ToolCallStatus::Done,
                    duration_ms: Some(50),
                    output: None,
                }],
                total_tool_calls: 3,
                tokens_in: 14_203,
                tokens_out: 8_441,
                files_touched: vec![FileTouched {
                    path: "src/lib.rs".into(),
                    kind: FileKind::Modified,
                }],
                turn_count: 0,
                session_id: String::new(),
                agent_version: String::new(),
                context_history: vec![],
                subagents: vec![],
            }),
            context_percent: None,
            compaction_count: 0,
            agent_cli: String::new(),
        };
        let bytes = encode_to_vec(&detail, standard()).unwrap();
        let (decoded, _): (AgentDetail, usize) = decode_from_slice(&bytes, standard()).unwrap();
        assert_eq!(decoded.duration_s, 120);
        let acp = decoded.acp.unwrap();
        assert_eq!(acp.tokens_in, 14_203);
        assert_eq!(acp.files_touched[0].kind, FileKind::Modified);
        assert_eq!(acp.current_tool.unwrap().status, ToolCallStatus::Running);
    }

    #[test]
    fn set_split_ratio_simple_split() {
        let mut layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
            ratio: 0.5,
        };
        assert!(layout.set_split_ratio(PaneId(1), PaneId(2), 0.3));
        match layout {
            PaneLayout::Split { ratio, .. } => assert!((ratio - 0.3).abs() < f32::EPSILON),
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn set_split_ratio_clamps_and_rejects_nan() {
        let mut layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
            ratio: 0.5,
        };
        assert!(layout.set_split_ratio(PaneId(1), PaneId(2), f32::NAN));
        match layout {
            PaneLayout::Split { ratio, .. } => {
                assert!(ratio.is_finite());
                assert!((0.1..=0.9).contains(&ratio));
            }
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn set_split_ratio_leaf_no_op() {
        let mut layout = PaneLayout::Leaf(PaneId(1));
        assert!(!layout.set_split_ratio(PaneId(1), PaneId(2), 0.3));
    }

    #[test]
    fn set_split_ratio_nested_inner_split() {
        let mut layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            first: Box::new(PaneLayout::Split {
                direction: SplitDir::Horizontal,
                first: Box::new(PaneLayout::Leaf(PaneId(1))),
                second: Box::new(PaneLayout::Leaf(PaneId(2))),
                ratio: 0.5,
            }),
            second: Box::new(PaneLayout::Leaf(PaneId(3))),
            ratio: 0.5,
        };
        assert!(layout.set_split_ratio(PaneId(1), PaneId(2), 0.7));
        match layout {
            PaneLayout::Split {
                first: inner,
                second,
                ratio: outer_ratio,
                ..
            } => {
                assert!((outer_ratio - 0.5).abs() < f32::EPSILON);
                assert_eq!(second.leaves(), vec![PaneId(3)]);
                match *inner {
                    PaneLayout::Split { ratio, .. } => {
                        assert!((ratio - 0.7).abs() < f32::EPSILON)
                    }
                    _ => panic!("expected inner split"),
                }
            }
            _ => panic!("expected outer split"),
        }
    }

    #[test]
    fn set_split_ratio_nested_outer_split() {
        let mut layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            first: Box::new(PaneLayout::Split {
                direction: SplitDir::Horizontal,
                first: Box::new(PaneLayout::Leaf(PaneId(1))),
                second: Box::new(PaneLayout::Leaf(PaneId(2))),
                ratio: 0.5,
            }),
            second: Box::new(PaneLayout::Leaf(PaneId(3))),
            ratio: 0.5,
        };
        assert!(layout.set_split_ratio(PaneId(1), PaneId(3), 0.2));
        match layout {
            PaneLayout::Split { ratio, .. } => {
                assert!((ratio - 0.2).abs() < f32::EPSILON)
            }
            _ => panic!("expected outer split"),
        }
    }
}
