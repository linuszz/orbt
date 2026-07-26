//! Wire protocol shared between the orbit TUI client and the orbtd daemon.
//!
//! All messages use length-prefixed bincode 2.x encoding. The serialization path
//! is `bincode::serde::encode_to_vec` / `bincode::serde::decode_from_slice` —
//! message types only derive `serde::Serialize/Deserialize`; do NOT derive
//! `bincode::Encode`.
//!
//! See `06_tech-design/03-ipc-protocol.md` for the full protocol spec.

pub mod encoding;
pub mod error;
pub mod messages;
pub mod socket;
pub mod types;

pub use encoding::{decode_message, encode_message, MAX_MSG_BYTES};
pub use error::ProtocolError;
pub use messages::{Capabilities, ClientMessage, ServerEvent, PROTOCOL_VERSION};
pub use socket::default_socket_path;
pub use types::{
    AcpDetail, AgentDetail, AgentId, AgentInfo, AgentLaunchRequest, AgentMetrics, AgentProtocol,
    AgentStatus, Cell, CellFlags, CellGrid, FileKind, FileTouched, FullState, ImageId, PaneId,
    PaneInfo, PaneLayout, ScrollbackLine, SpaceId, SpaceInfo, SplitDir, TabId, TabInfo, TermColor,
    ToolCall, ToolCallStatus,
};
