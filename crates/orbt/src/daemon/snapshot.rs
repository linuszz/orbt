//! Session snapshot: serialize workspace state to TOML on daemon shutdown,
//! restore it on daemon startup. File: `~/.orbt/sessions/session.toml`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use orbt_core::config::config_dir;

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub spaces: Vec<SpaceSnapshot>,
    pub active_space_id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpaceSnapshot {
    pub id: u32,
    pub name: String,
    pub cwd: String,
    pub tabs: Vec<TabSnapshot>,
    pub active_tab_id: u32,
    pub agents: Vec<AgentSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub id: u32,
    pub name: String,
    pub active_pane_id: u32,
    pub panes: Vec<PaneSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub id: u32,
    pub cwd: String,
    pub scrollback: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub name: String,
    pub launch_cmd: Option<String>,
    pub cwd: String,
}

pub fn snapshot_path() -> PathBuf {
    config_dir().join("sessions").join("session.toml")
}

pub fn save(snap: &SessionSnapshot) -> anyhow::Result<()> {
    let path = snapshot_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str = toml::to_string_pretty(snap)?;
    std::fs::write(&path, toml_str)?;
    Ok(())
}

/// Load the last saved snapshot from disk.
/// Called by Task 5 (session restore on daemon startup).
#[allow(dead_code)] // consumed by Task 5 (restore-on-startup)
pub fn load() -> anyhow::Result<Option<SessionSnapshot>> {
    let path = snapshot_path();
    if !path.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&path)?;
    let snap: SessionSnapshot = toml::from_str(&s)?;
    Ok(Some(snap))
}

/// Delete the snapshot file (called after successful restore so it isn't replayed).
/// Called by Task 5 (session restore on daemon startup).
#[allow(dead_code)] // consumed by Task 5 (restore-on-startup)
pub fn delete() {
    let _ = std::fs::remove_file(snapshot_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrip() {
        let snap = SessionSnapshot {
            active_space_id: 1,
            spaces: vec![SpaceSnapshot {
                id: 1,
                name: "dev".to_string(),
                cwd: "/home/user".to_string(),
                active_tab_id: 1,
                tabs: vec![TabSnapshot {
                    id: 1,
                    name: "main".to_string(),
                    active_pane_id: 1,
                    panes: vec![PaneSnapshot {
                        id: 1,
                        cwd: "/home/user".to_string(),
                        scrollback: vec!["$ ls".to_string()],
                    }],
                }],
                agents: vec![],
            }],
        };
        let s = toml::to_string_pretty(&snap).unwrap();
        let loaded: SessionSnapshot = toml::from_str(&s).unwrap();
        assert_eq!(loaded.spaces[0].name, "dev");
        assert_eq!(loaded.spaces[0].tabs[0].panes[0].scrollback[0], "$ ls");
        assert_eq!(loaded.active_space_id, 1);
    }

    #[test]
    fn snapshot_with_agent() {
        let snap = SessionSnapshot {
            active_space_id: 0,
            spaces: vec![SpaceSnapshot {
                id: 0,
                name: "orbital-mars".to_string(),
                cwd: "/tmp".to_string(),
                active_tab_id: 0,
                tabs: vec![],
                agents: vec![AgentSnapshot {
                    name: "claude".to_string(),
                    launch_cmd: Some("claude --print 'hello'".to_string()),
                    cwd: "/tmp".to_string(),
                }],
            }],
        };
        let s = toml::to_string_pretty(&snap).unwrap();
        let loaded: SessionSnapshot = toml::from_str(&s).unwrap();
        assert_eq!(loaded.spaces[0].agents[0].name, "claude");
        assert_eq!(
            loaded.spaces[0].agents[0].launch_cmd.as_deref(),
            Some("claude --print 'hello'")
        );
    }

    #[test]
    fn empty_snapshot_roundtrip() {
        let snap = SessionSnapshot {
            active_space_id: 0,
            spaces: vec![],
        };
        let s = toml::to_string_pretty(&snap).unwrap();
        let loaded: SessionSnapshot = toml::from_str(&s).unwrap();
        assert!(loaded.spaces.is_empty());
    }
}
