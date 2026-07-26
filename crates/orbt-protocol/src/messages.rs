//! IPC message envelopes. The full variant set is the wire contract between
//! `orbit` and `orbtd`. See `06_tech-design/03-ipc-protocol.md` §2.
//!
//! Versioning strategy:
//! - `PROTOCOL_VERSION` bumps on breaking changes (variant rename, field removal).
//! - `Capabilities` (forward-compatible bool flags) for additive features.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 4; // was 3; AgentDetail gained `acp` field

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capabilities {
    pub image_transfer: bool,
    pub file_transfer: bool,
    pub mcp_bridge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello {
        client_version: String,
        protocol_version: u32,
        capabilities: Capabilities,
    },

    CreateSpace {
        name: Option<String>,
    },
    SwitchSpace {
        space_id: crate::SpaceId,
    },
    CloseSpace {
        space_id: crate::SpaceId,
    },
    ReorderSpace {
        space_id: crate::SpaceId,
        to_index: usize,
    },

    SplitPane {
        tab_id: crate::TabId,
        pane_id: crate::PaneId,
        direction: crate::SplitDir,
    },
    ClosePane {
        tab_id: crate::TabId,
        pane_id: crate::PaneId,
    },
    ResizePane {
        tab_id: crate::TabId,
        pane_id: crate::PaneId,
        cols: u16,
        rows: u16,
    },
    FocusPane {
        tab_id: crate::TabId,
        pane_id: crate::PaneId,
    },

    NewTab {
        name: Option<String>,
    },
    CloseTab {
        tab_id: crate::TabId,
    },
    SwitchTab {
        tab_id: crate::TabId,
    },
    ReorderTab {
        tab_id: crate::TabId,
        to_index: usize,
    },
    ResizeSplit {
        tab_id: crate::TabId,
        first_pane: crate::PaneId,
        second_pane: crate::PaneId,
        ratio: f32,
    },

    PaneInput {
        tab_id: crate::TabId,
        pane_id: crate::PaneId,
        data: Vec<u8>,
    },

    AgentRespond {
        agent_id: crate::AgentId,
        response: String,
    },
    AgentSkip {
        agent_id: crate::AgentId,
    },
    AgentAbort {
        agent_id: crate::AgentId,
    },
    AgentRestart {
        agent_id: crate::AgentId,
    },
    AgentLaunch {
        config: crate::AgentLaunchRequest,
    },
    AgentRemove {
        agent_id: crate::AgentId,
    },

    RequestFullState,

    RequestScrollback {
        pane_id: crate::PaneId,
        from_seq: u64,
        count: u16,
    },

    CopyToClipboard {
        text: String,
    },

    // Phase 3: upload a local file (typically an image) to orbtd's payload store.
    // orbtd replies with PayloadReady containing the remote filesystem path.
    UploadPayload {
        data: Vec<u8>,
        filename: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerEvent {
    Welcome {
        server_version: String,
        protocol_version: u32,
        capabilities: Capabilities,
        state: crate::FullState,
    },
    ProtocolError {
        code: u32,
        message: String,
    },

    SpaceCreated(crate::SpaceInfo),
    SpaceClosed(crate::SpaceId),
    SpaceUpdated(crate::SpaceInfo),

    PaneOutput {
        pane_id: crate::PaneId,
        data: Vec<u8>,
    },
    PaneSnapshot {
        pane_id: crate::PaneId,
        cell_grid: crate::CellGrid,
    },

    AgentStatusChanged {
        agent_id: crate::AgentId,
        new_status: crate::AgentStatus,
        detail: Option<crate::AgentDetail>,
    },
    AgentCreated(crate::AgentInfo),
    AgentRemoved(crate::AgentId),
    AgentAcpUpdated {
        agent_id: crate::AgentId,
        data: crate::AcpDetail,
    },
    AgentMetricsUpdated {
        agent_id: crate::AgentId,
        metrics: crate::AgentMetrics,
    },

    ImageReady {
        image_id: crate::ImageId,
        pane_id: crate::PaneId,
        width_px: u32,
        height_px: u32,
        size_bytes: u32,
    },

    ScrollbackLines {
        pane_id: crate::PaneId,
        lines: Vec<crate::ScrollbackLine>,
        oldest_seq: u64,
    },

    Ping,

    // Phase 3: the uploaded payload is now available at `path` on the orbtd host.
    PayloadReady {
        path: String,
    },
}
