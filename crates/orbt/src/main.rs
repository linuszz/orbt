mod daemon;
mod events;
mod ipc;
mod ssh;

use anyhow::{Context, Result};
use orbt_protocol::ClientMessage;
use tracing::debug;

use crate::ipc::IpcClient;
use orbt_tui::app::{load_settings, App};
use orbt_tui::tui;

fn parse_remote_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--remote" {
            return iter.next().cloned();
        }
        if let Some(val) = arg.strip_prefix("--remote=") {
            return Some(val.to_string());
        }
    }
    None
}

// Attempt to connect to local orbtd. On failure, spawn `orbit daemon` as a
// background process, wait briefly for it to bind the socket, then retry once.
async fn connect_local_with_autostart() -> Result<(
    crate::ipc::IpcWriter,
    crate::ipc::IpcReader,
    orbt_protocol::FullState,
)> {
    if let Ok((ipc, state)) = IpcClient::connect().await {
        let (w, r) = ipc.into_split();
        return Ok((w, r, state));
    }

    debug!("orbtd not running, auto-starting daemon...");
    let exe = std::env::current_exe().context("cannot resolve orbit binary path")?;
    std::process::Command::new(&exe)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("failed to spawn orbit daemon")?;

    // Give the daemon time to bind the socket (up to 2 s in 100 ms steps).
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if let Ok((ipc, state)) = IpcClient::connect().await {
            let (w, r) = ipc.into_split();
            return Ok((w, r, state));
        }
    }

    // Final attempt — return a proper error if it still hasn't come up.
    let (ipc, state) = IpcClient::connect()
        .await
        .context("orbtd did not start in time — check logs with ORBT_LOG_LEVEL=debug")?;
    let (w, r) = ipc.into_split();
    Ok((w, r, state))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // `orbit daemon` — run the daemon in the foreground (used by servers and
    // by the auto-start path above when it forks itself).
    if args.get(1).map(|s| s.as_str()) == Some("daemon") {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_env("ORBT_LOG_LEVEL")
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with_ansi(false)
            .with_file(true)
            .with_line_number(true)
            .init();
        return daemon::run().await;
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("ORBT_LOG_LEVEL")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let (writer, reader, state) = if let Some(remote) = parse_remote_arg() {
        debug!("connecting to remote orbtd via SSH: {remote}");
        let spec = ssh::RemoteSpec::parse(&remote)
            .with_context(|| format!("invalid remote spec: {remote}"))?;
        ssh::connect_remote(&spec).await?
    } else {
        debug!("connecting to local orbtd...");
        connect_local_with_autostart().await?
    };

    debug!("setting up terminal...");
    let mut terminal = tui::setup_terminal().context("failed to setup terminal")?;

    let term_cols = terminal.size()?.width;
    let term_rows = terminal.size()?.height;

    let sidebar_w: u16 = if term_cols < 80 {
        tui::SIDEBAR_COLLAPSED_W
    } else {
        tui::SIDEBAR_W
    };
    let total_cols = term_cols.saturating_sub(sidebar_w).max(20);
    let total_rows = term_rows.saturating_sub(3).max(5);

    let mut app = App::from_welcome(&state, total_cols, total_rows);

    let settings = load_settings();
    app.theme_name = settings.theme.clone();
    app.sidebar_visible = settings.sidebar_visible;
    app.agent_fleet_enabled = settings.agent_fleet_enabled;
    app.agent_panel_mode = settings.agent_panel_mode;
    orbt_tui::tui::theme::set_theme(&app.theme_name);

    let pane_area = ratatui::layout::Rect {
        x: 0,
        y: 0,
        width: total_cols,
        height: total_rows,
    };
    let areas = tui::compute_leaf_areas(app.pane_tree(), pane_area);
    for (pid, rect) in areas {
        let pc = rect.width;
        let pr = rect.height.saturating_sub(2);
        if let Some(pane) = app.panes.get_mut(&pid) {
            pane.parser.grid.resize(pc, pr);
        }
        let _ = writer
            .send(ClientMessage::ResizePane {
                tab_id: app.active_tab_id,
                pane_id: pid,
                cols: pc,
                rows: pr,
            })
            .await;
    }

    let _ = writer.send(ClientMessage::RequestFullState).await;

    debug!("entering event loop");
    let run_result = events::run(&mut app, writer, reader, &mut terminal).await;

    let _ = tui::restore_terminal(&mut terminal);
    run_result
}
