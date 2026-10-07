use orbit_plugin::prelude::*;
use std::collections::HashMap;

struct NestedAgentPlugin {
    agents: Vec<AgentInfo>,
    config: PluginConfig,
}

#[derive(Debug, Clone)]
struct AgentInfo {
    name: String,
    status: String,
    task: Option<String>,
}

impl Plugin for NestedAgentPlugin {
    fn new() -> Self {
        Self {
            agents: Vec::new(),
            config: PluginConfig::empty(),
        }
    }

    fn init(&mut self, ctx: &mut InitContext) -> Result<(), PluginError> {
        ctx.declare_surface(SurfaceDecl {
            kind: SurfaceKind::PluginDock,
            id: "nested-agent-dock".to_string(),
            priority: 60,
            min_size: Some(Size { w: 200, h: 10 }),
            max_size: Some(Size { w: 400, h: 60 }),
            resize_policy: ResizePolicy::Stretch,
            config: None,
        })?;

        ctx.subscribe(HookSpec {
            topic: HookTopic::AgentState,
            filter: None,
            mode: HookMode::Edge,
            delivery: Delivery::Async,
            buffer: None,
            priority: 50,
        })?;

        ctx.declare_command_used(CommandName("agent.list".into()))?;
        ctx.declare_command_provided(CommandProvidedDecl {
            name: "nested-agent.intercept".to_string(),
            description: "Intercept nested agent output".to_string(),
            args_schema: None,
        })?;

        self.config = PluginConfig::from_schema(ctx.config_schema());
        self.load_agents_from_store(ctx);

        Ok(())
    }

    fn on_activate(&mut self, ctx: &mut RuntimeContext) {
        self.render_agent_list(ctx);
    }

    fn on_command(
        &mut self,
        ctx: &mut RuntimeContext,
        cmd_name: &str,
        _args: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, PluginError> {
        match cmd_name {
            "nested-agent.intercept" => {
                self.agents.clear();
                self.render_agent_list(ctx);
                Ok(Some(serde_json::json!({"status": "intercepted"})))
            }
            _ => Err(PluginError::CommandNotFound {
                name: cmd_name.into(),
            }),
        }
    }

    fn on_edge(&mut self, ctx: &mut RuntimeContext, event: EdgeEvent) {
        if event.topic == HookTopic::AgentState {
            if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&event.payload) {
                let name = payload
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let status = payload
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("idle")
                    .to_string();

                self.agents.retain(|a| a.name != name);
                self.agents.push(AgentInfo {
                    name,
                    status,
                    task: None,
                });

                self.render_agent_list(ctx);
            }
        }
    }
}

impl NestedAgentPlugin {
    fn load_agents_from_store(&mut self, ctx: &InitContext) {
        if let Ok(Some(data)) = ctx.store_read("agents") {
            if let Ok(agents) = serde_json::from_slice::<Vec<AgentInfo>>(&data) {
                self.agents = agents;
            }
        }
    }

    fn render_agent_list(&self, ctx: &mut RuntimeContext) {
        let surface_id = SurfaceId(0);
        let handle = ctx.surface(surface_id);

        let width = 40u16;
        let height = (self.agents.len() as u16 + 2).min(20);
        let mut cells = Vec::with_capacity((width * height) as usize);

        let header = " Nested Agents ";
        for (i, ch) in header.chars().enumerate() {
            if i < width as usize {
                cells.push(Cell {
                    ch,
                    fg: TermColor::Accent,
                    bg: TermColor::BgSecondary,
                    ..Default::default()
                });
            }
        }
        while cells.len() < width as usize {
            cells.push(Cell::blank(TermColor::BgSecondary));
        }

        for agent in &self.agents {
            let line = format!("  {} {}", status_icon(&agent.status), agent.name);
            for (i, ch) in line.chars().enumerate() {
                if i < width as usize {
                    let fg = match agent.status.as_str() {
                        "working" => TermColor::Accent,
                        "blocked" => TermColor::AccentBlocked,
                        "error" => TermColor::AccentError,
                        _ => TermColor::FgMuted,
                    };
                    cells.push(Cell {
                        ch,
                        fg,
                        bg: TermColor::BgSecondary,
                        ..Default::default()
                    });
                }
            }
            while cells.len() % width as usize != 0 {
                cells.push(Cell::blank(TermColor::BgSecondary));
            }
        }

        while cells.len() < (width * height) as usize {
            cells.push(Cell::blank(TermColor::BgSecondary));
        }

        let frame = Frame {
            size: Size { w: width, h: height },
            dirty: None,
            buffer: cells,
            cursor: None,
            interactives: vec![],
            target_client: None,
        };

        let _ = handle.render(frame);
    }
}

fn status_icon(status: &str) -> &'static str {
    match status {
        "working" => "●",
        "blocked" => "◎",
        "error" => "◉",
        _ => "○",
    }
}

export_plugin!(NestedAgentPlugin);
