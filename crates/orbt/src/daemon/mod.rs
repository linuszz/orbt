pub mod agent;
pub mod io;
pub mod ipc;
pub mod pty;
pub mod session;
pub mod snapshot;

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::{GenericFilePath, ListenerOptions};
use tokio::sync::broadcast;
use tracing::{error, info};

use self::session::SpaceManager;
use orbt_protocol::ServerEvent;

pub use orbt_protocol::default_socket_path;

fn lock_file_path() -> std::path::PathBuf {
    default_socket_path().with_extension("lock")
}

/// Atomically acquire the lock file using O_CREAT|O_EXCL semantics.
/// If the file already exists, checks whether the recorded PID is still alive.
/// On non-unix, a stale lock is always overwritten.
fn acquire_lock(path: &Path) -> Result<()> {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut f) => {
            write!(f, "{}", std::process::id())?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Stale lock? Check if the recorded PID is still alive.
            #[cfg(unix)]
            {
                let pid: u32 = std::fs::read_to_string(path)
                    .ok()
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);
                if pid > 0 && unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
                    anyhow::bail!("orbtd already running (PID {pid})");
                }
            }
            std::fs::write(path, std::process::id().to_string())?;
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

fn release_lock(path: &Path) {
    let _ = std::fs::remove_file(path);
}

pub async fn run() -> Result<()> {
    let lock_path = lock_file_path();
    acquire_lock(&lock_path).context("failed to acquire lock")?;

    let socket_path = default_socket_path();
    let name = socket_path
        .to_str()
        .context("socket path is not valid UTF-8")?
        .to_fs_name::<GenericFilePath>()
        .context("failed to create socket name")?;

    if socket_path.exists() {
        std::fs::remove_file(&socket_path).ok();
    }

    let listener = ListenerOptions::new()
        .name(name)
        .create_tokio()
        .with_context(|| format!("failed to bind socket at {}", socket_path.display()))?;
    info!("orbtd listening on {}", socket_path.display());

    let (event_bus, _rx) = broadcast::channel::<ServerEvent>(256);

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| ".".to_string());
    let space_manager = Arc::new(SpaceManager::new(event_bus, shell, cwd, 80, 24).await?);
    info!("orbtd ready — 1 space, 1 pane");

    {
        let sm = space_manager.clone();
        tokio::spawn(async move { sm.poll_cwd_changes(500).await });
    }

    tokio::select! {
        res = accept_loop(listener, space_manager.clone()) => {
            if let Err(e) = res { error!("accept loop error: {e:#}"); }
        }
        _ = wait_for_signal() => {
            info!("stopping orbtd");
            space_manager.save_snapshot().await;
            space_manager.shutdown_all().await;
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    release_lock(&lock_path);
    info!("orbtd stopped");
    Ok(())
}

async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = term.recv() => info!("SIGTERM received, shutting down"),
            _ = tokio::signal::ctrl_c() => info!("SIGINT received, shutting down"),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn accept_loop(
    listener: interprocess::local_socket::tokio::Listener,
    space_manager: Arc<SpaceManager>,
) -> Result<()> {
    loop {
        let stream = listener.accept().await?;
        let space_manager = space_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = ipc::handle_client(stream, space_manager).await {
                error!("client error: {e:#}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn lock_file_no_race_on_fresh_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.lock");
        acquire_lock(&path).expect("first acquire should succeed");
        // Second acquire on same path while this process is alive must fail.
        let result = acquire_lock(&path);
        assert!(result.is_err(), "second acquire should fail with live PID");
    }

    #[test]
    #[cfg(not(unix))]
    fn lock_file_stale_overwritten_on_non_unix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.lock");
        acquire_lock(&path).expect("first acquire should succeed");
        // On non-unix, stale lock is always overwritten.
        acquire_lock(&path).expect("stale lock is overwritten on non-unix");
    }
}
