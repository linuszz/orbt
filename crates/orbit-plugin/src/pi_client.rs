use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PiClientError {
    #[error("failed to spawn pi process: {0}")]
    SpawnFailed(String),

    #[error("pi process exited unexpectedly")]
    ProcessExited,

    #[error("rpc error: {0}")]
    RpcError(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("timeout waiting for response")]
    Timeout,
}

pub type Result<T> = std::result::Result<T, PiClientError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub session_name: Option<String>,
    pub no_session: bool,
    pub cwd: Option<String>,
    pub api_key: Option<String>,
}

impl Default for PiConfig {
    fn default() -> Self {
        Self {
            provider: None,
            model: None,
            thinking_level: None,
            session_name: None,
            no_session: true,
            cwd: None,
            api_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PiCommand {
    Prompt {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        streaming_behavior: Option<String>,
    },
    Steer {
        message: String,
    },
    FollowUp {
        message: String,
    },
    Abort,
    GetState,
    GetMessages,
    NewSession,
    SetModel {
        provider: String,
        model_id: String,
    },
    SetThinkingLevel {
        level: String,
    },
    GetAvailableModels,
    Compact,
    GetSessionStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiResponse {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub response_type: String,
    pub command: String,
    pub success: bool,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

pub enum PiMessage {
    Response(PiResponse),
    Event(PiEvent),
}

pub struct PiClient {
    child: Child,
    stdin: ChildStdin,
    command_rx: Receiver<PiMessage>,
    response_tx: Sender<PiMessage>,
    pending_commands: HashMap<String, Sender<PiResponse>>,
    event_listeners: Vec<Box<dyn Fn(&PiEvent) + Send>>,
}

impl PiClient {
    pub fn spawn(config: PiConfig) -> Result<Self> {
        let mut cmd = Command::new("pi");
        cmd.arg("--mode").arg("rpc");
        
        if config.no_session {
            cmd.arg("--no-session");
        }
        
        if let Some(provider) = &config.provider {
            cmd.arg("--provider").arg(provider);
        }
        
        if let Some(model) = &config.model {
            cmd.arg("--model").arg(model);
        }
        
        if let Some(thinking) = &config.thinking_level {
            cmd.arg("--thinking").arg(thinking);
        }
        
        if let Some(name) = &config.session_name {
            cmd.arg("--name").arg(name);
        }
        
        if let Some(cwd) = &config.cwd {
            cmd.current_dir(cwd);
        }
        
        if let Some(api_key) = &config.api_key {
            cmd.arg("--api-key").arg(api_key);
        }
        
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        
        let mut child = cmd.spawn().map_err(|e| {
            PiClientError::SpawnFailed(format!("{}: {}", e, "is pi installed and in PATH?"))
        })?;
        
        let stdin = child.stdin.take().ok_or(PiClientError::ProcessExited)?;
        let stdout = child.stdout.take().ok_or(PiClientError::ProcessExited)?;
        
        let (response_tx, command_rx) = channel();
        
        let reader_tx = response_tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if let Ok(msg) = serde_json::from_str::<PiResponse>(&line) {
                            let _ = reader_tx.send(PiMessage::Response(msg));
                        } else if let Ok(event) = serde_json::from_str::<PiEvent>(&line) {
                            let _ = reader_tx.send(PiMessage::Event(event));
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        
        Ok(Self {
            child,
            stdin,
            command_rx,
            response_tx,
            pending_commands: HashMap::new(),
            event_listeners: Vec::new(),
        })
    }
    
    pub fn send_command(&mut self, cmd: PiCommand) -> Result<()> {
        let mut line = serde_json::to_string(&cmd)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;
        Ok(())
    }
    
    pub fn send_prompt(&mut self, message: &str) -> Result<()> {
        let id = format!("{:x}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0));
        self.send_command(PiCommand::Prompt {
            id: Some(id),
            message: message.to_string(),
            streaming_behavior: None,
        })
    }
    
    pub fn send_steer(&mut self, message: &str) -> Result<()> {
        self.send_command(PiCommand::Steer {
            message: message.to_string(),
        })
    }
    
    pub fn abort(&mut self) -> Result<()> {
        self.send_command(PiCommand::Abort)
    }
    
    pub fn poll_message(&mut self) -> Option<PiMessage> {
        match self.command_rx.try_recv() {
            Ok(msg) => {
                match &msg {
                    PiMessage::Response(resp) => {
                        if let Some(id) = &resp.id {
                            if let Some(tx) = self.pending_commands.remove(id) {
                                let _ = tx.send(resp.clone());
                            }
                        }
                    }
                    PiMessage::Event(event) => {
                        for listener in &self.event_listeners {
                            listener(event);
                        }
                    }
                }
                Some(msg)
            }
            Err(_) => None,
        }
    }
    
    pub fn add_event_listener<F>(&mut self, listener: F)
    where
        F: Fn(&PiEvent) + Send + 'static,
    {
        self.event_listeners.push(Box::new(listener));
    }
    
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
    
    pub fn shutdown(&mut self) -> Result<()> {
        let _ = self.stdin.flush();
        let _ = self.child.wait();
        Ok(())
    }
}

impl Drop for PiClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn pi_config_default() {
        let config = PiConfig::default();
        assert!(config.no_session);
        assert!(config.provider.is_none());
        assert!(config.model.is_none());
    }
    
    #[test]
    fn pi_command_serialization() {
        let cmd = PiCommand::Prompt {
            id: Some("test-1".to_string()),
            message: "Hello".to_string(),
            streaming_behavior: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"prompt\""));
        assert!(json.contains("\"message\":\"Hello\""));
    }
    
    #[test]
    fn pi_response_deserialization() {
        let json = r#"{
            "id": "req-1",
            "type": "response",
            "command": "prompt",
            "success": true,
            "data": {"disposition": "started"}
        }"#;
        let resp: PiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some("req-1".to_string()));
        assert!(resp.success);
        assert_eq!(resp.command, "prompt");
    }
    
    #[test]
    fn pi_event_deserialization() {
        let json = r#"{
            "type": "message_update",
            "assistantMessageEvent": {
                "type": "text_delta",
                "contentIndex": 0,
                "delta": "Hello"
            }
        }"#;
        let event: PiEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "message_update");
    }
}
