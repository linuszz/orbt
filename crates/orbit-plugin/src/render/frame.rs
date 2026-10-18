use orbt_protocol::Cell;

#[derive(Debug, Clone)]
pub struct Frame {
    pub size: Size,
    pub dirty: Option<Rect>,
    pub buffer: Vec<Cell>,
    pub cursor: Option<Cursor>,
    pub interactives: Vec<InteractiveRegion>,
    pub target_client: Option<ClientId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub w: u16,
    pub h: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub x: u16,
    pub y: u16,
    pub kind: CursorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Steady,
    Blinking,
    Hidden,
}

#[derive(Debug, Clone)]
pub struct InteractiveRegion {
    pub rect: Rect,
    pub action: InteractiveAction,
    pub hover_text: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InteractiveAction {
    Click { command: String },
    Key { key: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub u64);
