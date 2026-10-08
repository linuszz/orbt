use orbit_plugin::prelude::*;
use orbit_plugin::export_plugin;
use orbit_plugin::pi_client::{PiClient, PiConfig, PiEvent, PiMessage};
use std::collections::VecDeque;

const MAX_MESSAGES: usize = 100;
const SURFACE_WIDTH: u16 = 44;
const SURFACE_HEIGHT: u16 = 20;

struct NestedAgentPlugin {
    pi_client: Option<PiClient>,
    messages: VecDeque<ChatMessage>,
    input_buffer: String,
    surface_id: Option<SurfaceId>,
    status: AgentStatus,
    config: NestedAgentConfig,
}

#[derive(Debug, Clone)]
struct ChatMessage {
    role: MessageRole,
    content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentStatus {
    Idle,
    Running,
    Error,
}

#[derive(Debug, Clone)]
struct NestedAgentConfig {
    provider: String,
    model: String,
    thinking_level: String,
    max_tokens: u32,
    workspace: String,
}

impl Default for NestedAgentConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            thinking_level: "medium".to_string(),
            max_tokens: 16384,
            workspace: ".".to_string(),
        }
    }
}

impl Plugin for NestedAgentPlugin {
    fn new() -> Self {
        Self {
            pi_client: None,
            messages: VecDeque::new(),
            input_buffer: String::new(),
            surface_id: None,
            status: AgentStatus::Idle,
            config: NestedAgentConfig::default(),
        }
    }

    fn init(&mut self, ctx: &mut InitContext) -> Result<(), PluginError> {
        // Load config from plugin config
        self.config = load_config(ctx);
        
        // Declare plugin-dock surface
        let surface_id = ctx.declare_surface(SurfaceDecl {
            kind: SurfaceKind::PluginDock,
            id: "nested-agent-dock".to_string(),
            priority: 60,
            min_size: Some(Size { w: SURFACE_WIDTH, h: 10 }),
            max_size: Some(Size { w: SURFACE_WIDTH, h: SURFACE_HEIGHT }),
            resize_policy: ResizePolicy::Stretch,
            config: None,
        })?;
        self.surface_id = Some(surface_id);

        // Subscribe to agent state events
        ctx.subscribe(HookSpec {
            topic: HookTopic::AgentState,
            filter: None,
            mode: HookMode::Edge,
            delivery: Delivery::Async,
            buffer: None,
            priority: 50,
        })?;

        // Declare commands
        ctx.declare_command_used(CommandName("agent.list".into()))?;
        ctx.declare_command_provided(CommandProvidedDecl {
            name: "nested-agent.intercept".to_string(),
            description: "Send message to nested agent".to_string(),
            args_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                },
                "required": ["message"]
            })),
        })?;

        // Add system message
        self.messages.push_back(ChatMessage {
            role: MessageRole::System,
            content: "Nested Agent ready. Type a message to start.".to_string(),
        });

        Ok(())
    }

    fn on_activate(&mut self, ctx: &mut RuntimeContext) {
        self.render(ctx);
    }

    fn on_command(
        &mut self,
        ctx: &mut RuntimeContext,
        cmd_name: &str,
        args: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, PluginError> {
        match cmd_name {
            "nested-agent.intercept" => {
                let message = args
                    .get("message")
                    .and_then(|m| m.as_str())
                    .ok_or_else(|| PluginError::CommandInvalidArgs {
                        name: cmd_name.into(),
                        detail: "missing 'message' field".to_string(),
                    })?;
                
                self.handle_user_message(ctx, message);
                Ok(Some(serde_json::json!({"status": "sent"})))
            }
            _ => Err(PluginError::CommandNotFound {
                name: cmd_name.into(),
            }),
        }
    }

    fn on_surface_event(
        &mut self,
        ctx: &mut RuntimeContext,
        surface_id: SurfaceId,
        event: SurfaceEvent,
    ) {
        if self.surface_id != Some(surface_id) {
            return;
        }

        match event {
            SurfaceEvent::Key { key } => {
                self.handle_key(ctx, &key);
            }
            SurfaceEvent::Click { x, y, .. } => {
                self.handle_click(ctx, x, y);
            }
            _ => {}
        }
    }

    fn on_deactivate(&mut self, _ctx: &mut RuntimeContext) {
        if let Some(mut client) = self.pi_client.take() {
            let _ = client.shutdown();
        }
    }

    fn shutdown(&mut self) {
        if let Some(mut client) = self.pi_client.take() {
            let _ = client.shutdown();
        }
    }
}

impl NestedAgentPlugin {
    fn handle_user_message(&mut self, ctx: &mut RuntimeContext, message: &str) {
        self.messages.push_back(ChatMessage {
            role: MessageRole::User,
            content: message.to_string(),
        });
        
        if self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }

        // Start PI if not running
        if self.pi_client.is_none() {
            self.start_pi(ctx);
        }

        // Send to PI
        if let Some(client) = &mut self.pi_client {
            if let Err(e) = client.send_prompt(message) {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!("Error sending to PI: {}", e),
                });
                self.status = AgentStatus::Error;
            } else {
                self.status = AgentStatus::Running;
            }
        }

        self.render(ctx);
    }

    fn start_pi(&mut self, _ctx: &mut RuntimeContext) {
        let config = PiConfig {
            provider: Some(self.config.provider.clone()),
            model: Some(self.config.model.clone()),
            thinking_level: Some(self.config.thinking_level.clone()),
            session_name: Some("orbit-nested-agent".to_string()),
            no_session: true,
            cwd: Some(self.config.workspace.clone()),
            api_key: None,
        };

        match PiClient::spawn(config) {
            Ok(mut client) => {
                client.add_event_listener(|event| {
                    // Handle PI events in a real implementation
                    // This would need a channel to communicate back to the plugin
                });
                self.pi_client = Some(client);
                self.status = AgentStatus::Idle;
            }
            Err(e) => {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!("Failed to start PI: {}", e),
                });
                self.status = AgentStatus::Error;
            }
        }
    }

    fn handle_key(&mut self, ctx: &mut RuntimeContext, key: &KeySpec) {
        match key.key.as_str() {
            "Enter" => {
                if !self.input_buffer.is_empty() {
                    let message = self.input_buffer.clone();
                    self.input_buffer.clear();
                    self.handle_user_message(ctx, &message);
                }
            }
            "Backspace" => {
                self.input_buffer.pop();
                self.render(ctx);
            }
            "Escape" => {
                if let Some(client) = &mut self.pi_client {
                    let _ = client.abort();
                }
                self.status = AgentStatus::Idle;
                self.render(ctx);
            }
            c if c.len() == 1 => {
                self.input_buffer.push_str(c);
                self.render(ctx);
            }
            _ => {}
        }
    }

    fn handle_click(&mut self, ctx: &mut RuntimeContext, _x: u16, y: u16) {
        // Handle clicks on message history or buttons
        let _ = y;
        self.render(ctx);
    }

    fn render(&self, ctx: &mut RuntimeContext) {
        let Some(surface_id) = self.surface_id else {
            return;
        };

        let handle = ctx.surface(surface_id);
        let frame = self.build_frame();
        let _ = handle.render(frame);
    }

    fn build_frame(&self) -> Frame {
        let mut cells = Vec::with_capacity((SURFACE_WIDTH * SURFACE_HEIGHT) as usize);
        
        let header = match self.status {
            AgentStatus::Idle => " Nested Agent [Ready] ",
            AgentStatus::Running => " Nested Agent [Running...] ",
            AgentStatus::Error => " Nested Agent [Error] ",
        };
        self.push_line(&mut cells, header, TermColor::Rgb(255, 140, 66), TermColor::Rgb(30, 30, 40));
        
        self.push_line(&mut cells, &"─".repeat(SURFACE_WIDTH as usize), TermColor::Rgb(60, 60, 80), TermColor::Rgb(30, 30, 40));

        let visible_rows = SURFACE_HEIGHT.saturating_sub(4) as usize;
        let messages_to_show: Vec<_> = self.messages.iter().rev().take(visible_rows).collect();
        
        for msg in messages_to_show.iter().rev() {
            let (prefix, color) = match msg.role {
                MessageRole::User => ("> ", TermColor::Rgb(255, 140, 66)),
                MessageRole::Assistant => ("  ", TermColor::Rgb(220, 220, 230)),
                MessageRole::System => ("  ", TermColor::Rgb(140, 140, 150)),
            };
            
            let line = format!("{}{}", prefix, truncate(&msg.content, SURFACE_WIDTH as usize - prefix.len() - 1));
            self.push_line(&mut cells, &line, color, TermColor::Rgb(30, 30, 40));
        }

        while cells.len() < (SURFACE_WIDTH * (SURFACE_HEIGHT - 2)) as usize {
            cells.push(Cell { ch: ' ', fg: TermColor::Default, bg: TermColor::Rgb(30, 30, 40), flags: Default::default() });
        }

        let input_line = format!("> {}", truncate(&self.input_buffer, SURFACE_WIDTH as usize - 3));
        self.push_line(&mut cells, &input_line, TermColor::Rgb(220, 220, 230), TermColor::Rgb(40, 40, 55));

        cells.truncate((SURFACE_WIDTH * SURFACE_HEIGHT) as usize);
        while cells.len() < (SURFACE_WIDTH * SURFACE_HEIGHT) as usize {
            cells.push(Cell { ch: ' ', fg: TermColor::Default, bg: TermColor::Rgb(30, 30, 40), flags: Default::default() });
        }

        Frame {
            size: orbit_plugin::render::Size { w: SURFACE_WIDTH, h: SURFACE_HEIGHT },
            dirty: None,
            buffer: cells,
            cursor: Some(Cursor {
                x: 2 + self.input_buffer.len() as u16,
                y: SURFACE_HEIGHT - 1,
                kind: CursorKind::Blinking,
            }),
            interactives: vec![],
            target_client: None,
        }
    }

    fn push_line(&self, cells: &mut Vec<Cell>, text: &str, fg: TermColor, bg: TermColor) {
        for ch in text.chars() {
            cells.push(Cell { ch, fg, bg, flags: Default::default() });
        }
        while cells.len() % SURFACE_WIDTH as usize != 0 {
            cells.push(Cell { ch: ' ', fg: TermColor::Default, bg, flags: Default::default() });
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        truncated.push('…');
        truncated
    }
}

fn load_config(_ctx: &InitContext) -> NestedAgentConfig {
    NestedAgentConfig::default()
}

export_plugin!(NestedAgentPlugin);
