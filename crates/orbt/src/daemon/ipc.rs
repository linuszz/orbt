use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use arboard::Clipboard;
use orbt_protocol::{ClientMessage, ServerEvent, PROTOCOL_VERSION};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::debug;

use super::session::SpaceManager;

async fn write_msg<W: AsyncWrite + Unpin>(stream: &mut W, msg: &ServerEvent) -> Result<()> {
    let bytes = orbt_protocol::encode_message(msg)?;
    stream.write_all(&bytes).await?;
    Ok(())
}

async fn read_msg<R: AsyncRead + Unpin>(stream: &mut R) -> Result<ClientMessage> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > orbt_protocol::MAX_MSG_BYTES {
        bail!("message too large: {len}");
    }
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;
    let (msg, _) = orbt_protocol::decode_message(&data)?;
    Ok(msg)
}

pub async fn handle_client<S>(mut stream: S, space_manager: Arc<SpaceManager>) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    match read_msg(&mut stream).await {
        Ok(ClientMessage::Hello {
            protocol_version, ..
        }) => {
            if protocol_version != PROTOCOL_VERSION {
                let _ = write_msg(
                    &mut stream,
                    &ServerEvent::ProtocolError {
                        code: 1,
                        message: format!(
                            "version mismatch: client={protocol_version}, server={PROTOCOL_VERSION}"
                        ),
                    },
                )
                .await;
                bail!("protocol version mismatch");
            }
        }
        _ => bail!("expected Hello"),
    }

    write_msg(
        &mut stream,
        &ServerEvent::Welcome {
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
            capabilities: orbt_protocol::Capabilities::default(),
            state: space_manager.collect_full_state().await,
        },
    )
    .await?;

    // Nudge all PTY children with SIGWINCH so idle TUI apps (Claude Code, vim,
    // yazi) redraw and emit fresh output. Without this, a frozen cursor_visible=false
    // snapshot from the Welcome state never gets corrected until the user types.
    // Small delay lets the client process Welcome before the flood of PaneOutput.
    let sm = Arc::clone(&space_manager);
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        sm.nudge_all_spaces().await;
    });

    let mut rx = space_manager.event_bus.subscribe();

    loop {
        tokio::select! {
            biased;
            msg = read_msg(&mut stream) => {
                let msg = match msg { Ok(m) => m, Err(e) => { debug!("client read: {e:#}"); break; } };
                match msg {
                    ClientMessage::PaneInput { tab_id, pane_id, data } => {
                        let session = space_manager.active_session().await;
                        session.send_input(tab_id, pane_id, data).await;
                    }
                    ClientMessage::ClosePane { tab_id, pane_id } => {
                        let session = space_manager.active_session().await;
                        session.close_pane(tab_id, pane_id).await;
                    }
                    ClientMessage::SplitPane { tab_id, direction, .. } => {
                        let session = space_manager.active_session().await;
                        if let Err(e) = session.split_pane(tab_id, direction).await {
                            tracing::warn!("split: {e:#}");
                        }
                    }
                    ClientMessage::ResizePane { tab_id, pane_id, cols, rows } => {
                        let session = space_manager.active_session().await;
                        session.resize_pane(tab_id, pane_id, cols, rows).await;
                    }
                    ClientMessage::FocusPane { tab_id, pane_id } => {
                        let session = space_manager.active_session().await;
                        session.focus_pane(tab_id, pane_id).await;
                    }
                    ClientMessage::NewTab { name } => {
                        let session = space_manager.active_session().await;
                        if let Err(e) = session.new_tab(name).await {
                            tracing::warn!("new_tab: {e:#}");
                        }
                    }
                    ClientMessage::CloseTab { tab_id } => {
                        let session = space_manager.active_session().await;
                        session.close_tab(tab_id).await;
                    }
                    ClientMessage::SwitchTab { tab_id } => {
                        let session = space_manager.active_session().await;
                        session.switch_tab(tab_id).await;
                    }
                    ClientMessage::ReorderTab { tab_id, to_index } => {
                        let session = space_manager.active_session().await;
                        session.reorder_tab(tab_id, to_index).await;
                    }
                    ClientMessage::ResizeSplit { tab_id, first_pane, second_pane, ratio } => {
                        let session = space_manager.active_session().await;
                        session.resize_split(tab_id, first_pane, second_pane, ratio).await;
                    }
                    ClientMessage::RequestFullState => {
                        let s = space_manager.collect_full_state().await;
                        let _ = write_msg(&mut stream, &ServerEvent::Welcome {
                            server_version: env!("CARGO_PKG_VERSION").to_string(),
                            protocol_version: PROTOCOL_VERSION,
                            capabilities: orbt_protocol::Capabilities::default(),
                            state: s,
                        }).await;
                    }
                    ClientMessage::CopyToClipboard { text } => {
                        if let Ok(mut cb) = Clipboard::new() {
                            let _ = cb.set_text(text);
                        }
                    }
                    ClientMessage::CreateSpace { name } => {
                        if let Err(e) = space_manager.create_space(name).await {
                            tracing::warn!("create_space: {e:#}");
                        }
                    }
                    ClientMessage::SwitchSpace { space_id } => {
                        if let Err(e) = space_manager.switch_space(space_id).await {
                            tracing::warn!("switch_space: {e:#}");
                        }
                    }
                    ClientMessage::CloseSpace { space_id } => {
                        if let Err(e) = space_manager.close_space(space_id).await {
                            tracing::warn!("close_space: {e:#}");
                        }
                    }
                    ClientMessage::ReorderSpace { space_id, to_index } => {
                        space_manager.reorder_space(space_id, to_index).await;
                    }
                    ClientMessage::AgentAbort { agent_id } => {
                        space_manager.agent_registry.abort_agent(agent_id).await;
                    }
                    ClientMessage::AgentRestart { agent_id } => {
                        let registry = Arc::clone(&space_manager.agent_registry);
                        let sm = Arc::clone(&space_manager);
                        tokio::spawn(async move {
                            // SIGTERM the existing process.
                            registry.abort_agent(agent_id).await;
                            // Brief pause to allow the process to exit before resetting state.
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            // Reset agent status to Idle.
                            registry.restart_agent(agent_id).await;
                            // Re-launch if a launch command was recorded for this agent.
                            let Some(info) = registry.get_agent_info(agent_id).await else {
                                return;
                            };
                            if let (Some(cmd), Some(pane_id)) = (info.launch_cmd, info.pane_id) {
                                let Some(sess) = sm.get_session_for_pane(pane_id).await else {
                                    return;
                                };
                                tokio::time::sleep(Duration::from_millis(500)).await;
                                let bytes = format!("{}\r", cmd.trim()).into_bytes();
                                sess.send_input(orbt_protocol::TabId(u32::MAX), pane_id, bytes)
                                    .await;
                            }
                        });
                    }
                    ClientMessage::AgentRemove { agent_id } => {
                        space_manager.agent_registry.remove_agent(agent_id).await;
                    }
                    ClientMessage::AgentSkip { agent_id } => {
                        let agents = space_manager.agent_registry.get_agents().await;
                        if let Some(agent) = agents.iter().find(|a| a.id == agent_id) {
                            if let Some(pane_id) = agent.pane_id {
                                let session = space_manager.active_session().await;
                                let active_tab_id = *session.active_tab.read().await;
                                session.send_input(active_tab_id, pane_id, b"\r".to_vec()).await;
                            }
                        }
                    }
                    ClientMessage::AgentRespond { agent_id, response } => {
                        let agents = space_manager.agent_registry.get_agents().await;
                        if let Some(agent) = agents.iter().find(|a| a.id == agent_id) {
                            if let Some(pane_id) = agent.pane_id {
                                let session = space_manager.active_session().await;
                                let active_tab_id = *session.active_tab.read().await;
                                let input = format!("{}\r", response.trim());
                                session.send_input(active_tab_id, pane_id, input.into_bytes()).await;
                            }
                        }
                    }
                    ClientMessage::AgentLaunch { config } => {
                        let session = space_manager.active_session().await;
                        let active_tab_id = *session.active_tab.read().await;
                        match session.split_pane(active_tab_id, orbt_protocol::SplitDir::Horizontal).await {
                            Ok(new_pane_id) => {
                                // Record the launch command so AgentRestart can re-execute it.
                                space_manager
                                    .agent_registry
                                    .set_pending_launch_cmd(new_pane_id, config.name.clone())
                                    .await;
                                let cmd = format!("{}\r", config.name.trim());
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_millis(300)).await;
                                    session.send_input(active_tab_id, new_pane_id, cmd.into_bytes()).await;
                                });
                            }
                            Err(e) => tracing::warn!("AgentLaunch split failed: {e:#}"),
                        }
                    }
                    ClientMessage::UploadPayload { data, filename } => {
                        if let Some(path) = save_payload(data, &filename).await {
                            let _ = write_msg(&mut stream, &ServerEvent::PayloadReady { path }).await;
                        }
                    }
                    _ => {}
                }
            }
            recv = rx.recv() => {
                let event = match recv {
                    Ok(e) => e,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => { debug!("lagged {n}"); continue; }
                    Err(_) => break,
                };
                if write_msg(&mut stream, &event).await.is_err() { break; }
            }
        }
    }
    debug!("client disconnected");
    Ok(())
}

async fn save_payload(data: Vec<u8>, filename: &str) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let dir = PathBuf::from(home).join(".orbt").join("payloads");
    tokio::fs::create_dir_all(&dir).await.ok()?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let path = dir.join(format!("{ts}.{ext}"));
    tokio::fs::write(&path, &data).await.ok()?;
    Some(path.to_string_lossy().into_owned())
}
