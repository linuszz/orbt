use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

#[derive(Debug, Error)]
pub enum AcpError {
    #[error("failed to spawn process: {0}")]
    SpawnFailed(String),

    #[error("process exited unexpectedly")]
    ProcessExited,

    #[error("JSON-RPC error: code={code}, message={message}")]
    RpcError { code: i64, message: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("timeout waiting for response")]
    Timeout,

    #[error("unexpected response: {0}")]
    UnexpectedResponse(String),
}

pub type Result<T> = std::result::Result<T, AcpError>;

#[derive(Debug, Clone)]
pub struct AcpClientConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
}

impl Default for AcpClientConfig {
    fn default() -> Self {
        Self {
            command: "opencode".to_string(),
            args: vec!["acp".to_string()],
            cwd: None,
            env: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AcpEvent {
    SessionUpdate {
        session_id: String,
        update: crate::types::SessionUpdate,
    },
    AgentRequest {
        id: u64,
        method: String,
        params: serde_json::Value,
    },
}

pub struct AcpClient {
    child: Child,
    stdin: ChildStdin,
    next_id: Arc<AtomicU64>,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<serde_json::Value>>>>,
    event_tx: mpsc::Sender<AcpEvent>,
    event_rx: mpsc::Receiver<AcpEvent>,
    reader_handle: Option<JoinHandle<()>>,
}

impl AcpClient {
    pub async fn spawn(config: AcpClientConfig) -> Result<Self> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        if let Some(cwd) = &config.cwd {
            cmd.current_dir(cwd);
        }

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| {
            AcpError::SpawnFailed(format!(
                "{}: {} (is {} installed and in PATH?)",
                e, config.command, config.command
            ))
        })?;

        let stdin = child.stdin.take().ok_or(AcpError::ProcessExited)?;
        let stdout = child.stdout.take().ok_or(AcpError::ProcessExited)?;

        let (event_tx, event_rx) = mpsc::channel(256);
        let pending: Arc<Mutex<HashMap<u64, mpsc::Sender<serde_json::Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let reader_pending = pending.clone();
        let reader_events = event_tx.clone();

        let reader_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            loop {
                match lines.next() {
                    Some(Ok(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }

                        if let Ok(response) = serde_json::from_str::<JsonRpcResponseWire>(&line) {
                            if response.id.is_some() {
                                let id = response.id.unwrap();
                                let mut pending_guard = reader_pending.lock().await;
                                if let Some(tx) = pending_guard.remove(&id) {
                                    let value = if let Some(err) = response.error {
                                        serde_json::json!({"error": err})
                                    } else {
                                        response.result.unwrap_or(serde_json::Value::Null)
                                    };
                                    let _ = tx.send(value).await;
                                }
                                continue;
                            }
                        }

                        if let Ok(notification) =
                            serde_json::from_str::<JsonRpcNotificationWire>(&line)
                        {
                            if notification.method == "session/update" {
                                if let Ok(params) = serde_json::from_value::<
                                    crate::types::SessionUpdateParams,
                                >(
                                    notification.params
                                ) {
                                    let _ = reader_events
                                        .send(AcpEvent::SessionUpdate {
                                            session_id: params.session_id,
                                            update: params.update,
                                        })
                                        .await;
                                }
                            } else {
                                let _ = reader_events
                                    .send(AcpEvent::AgentRequest {
                                        id: 0,
                                        method: notification.method,
                                        params: notification.params,
                                    })
                                    .await;
                            }
                            continue;
                        }

                        if let Ok(request) = serde_json::from_str::<JsonRpcRequestWire>(&line) {
                            let _ = reader_events
                                .send(AcpEvent::AgentRequest {
                                    id: request.id,
                                    method: request.method,
                                    params: request.params,
                                })
                                .await;
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            next_id: Arc::new(AtomicU64::new(1)),
            pending,
            event_tx,
            event_rx,
            reader_handle: Some(reader_handle),
        })
    }

    pub async fn initialize(&mut self) -> Result<crate::types::InitializeResult> {
        let params = crate::types::InitializeParams {
            protocol_version: crate::types::ACP_PROTOCOL_VERSION,
            client_capabilities: crate::types::ClientCapabilities {
                fs: Some(crate::types::FsCapabilities {
                    read_text_file: true,
                    write_text_file: true,
                }),
                terminal: Some(true),
            },
        };

        let result = self
            .request("initialize", serde_json::to_value(&params)?)
            .await?;
        let init_result: crate::types::InitializeResult = serde_json::from_value(result)?;
        Ok(init_result)
    }

    pub async fn new_session(&mut self, cwd: Option<String>) -> Result<String> {
        let params = crate::types::NewSessionParams { cwd, mcp_servers: None };
        let result = self
            .request("session/new", serde_json::to_value(&params)?)
            .await?;
        let session_result: crate::types::NewSessionResult = serde_json::from_value(result)?;
        Ok(session_result.session_id)
    }

    pub async fn load_session(&mut self, session_id: &str, cwd: Option<String>) -> Result<String> {
        let params = crate::types::LoadSessionParams {
            session_id: session_id.to_string(),
            cwd,
        };
        let result = self
            .request("session/load", serde_json::to_value(&params)?)
            .await?;
        let session_result: crate::types::LoadSessionResult = serde_json::from_value(result)?;
        Ok(session_result.session_id)
    }

    pub async fn prompt(&mut self, session_id: &str, text: &str) -> Result<String> {
        let params = crate::types::PromptParams {
            session_id: session_id.to_string(),
            prompt: vec![crate::types::ContentBlock::Text {
                text: text.to_string(),
            }],
        };
        let result = self
            .request("session/prompt", serde_json::to_value(&params)?)
            .await?;
        let prompt_result: crate::types::PromptResult = serde_json::from_value(result)?;
        Ok(prompt_result.stop_reason)
    }

    pub async fn cancel(&mut self, session_id: &str) -> Result<()> {
        let params = crate::types::CancelParams {
            session_id: session_id.to_string(),
        };
        self.request("session/cancel", serde_json::to_value(&params)?)
            .await?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> Option<AcpEvent> {
        self.event_rx.recv().await
    }

    pub fn try_next_event(&mut self) -> Option<AcpEvent> {
        self.event_rx.try_recv().ok()
    }

    async fn request(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequestWire {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let mut line = serde_json::to_string(&request)?;
        line.push('\n');

        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;

        let (tx, mut rx) = mpsc::channel(1);
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        match tokio::time::timeout(std::time::Duration::from_secs(30), rx.recv()).await {
            Ok(Some(value)) => {
                if let Some(error) = value.get("error") {
                    let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
                    let message = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error")
                        .to_string();
                    return Err(AcpError::RpcError { code, message });
                }
                Ok(value)
            }
            Ok(None) => Err(AcpError::ProcessExited),
            Err(_) => Err(AcpError::Timeout),
        }
    }

    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.stdin.flush();
        let _ = self.child.wait();
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.await;
        }
        Ok(())
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequestWire {
    jsonrpc: String,
    id: u64,
    method: String,
    params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponseWire {
    jsonrpc: String,
    id: Option<u64>,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcNotificationWire {
    jsonrpc: String,
    method: String,
    params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequestFromAgent {
    jsonrpc: String,
    id: u64,
    method: String,
    params: serde_json::Value,
}
