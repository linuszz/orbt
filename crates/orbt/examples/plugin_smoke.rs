use orbt_protocol::{Capabilities, ClientMessage, ServerEvent, PROTOCOL_VERSION};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

fn socket_path() -> std::path::PathBuf {
    let uid = unsafe { libc::getuid() };
    let run = std::path::PathBuf::from(format!("/run/user/{}/orbt.sock", uid));
    if run.exists() {
        return run;
    }
    std::path::PathBuf::from(format!("/tmp/orbt-{}.sock", uid))
}

async fn write_msg(stream: &mut UnixStream, msg: &ClientMessage) -> std::io::Result<()> {
    let bytes = orbt_protocol::encode_message(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    stream.write_all(&bytes).await
}

async fn read_msg(stream: &mut UnixStream) -> std::io::Result<ServerEvent> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;
    let (msg, _) = orbt_protocol::decode_message(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    Ok(msg)
}

fn frame_text(cells: &[orbt_protocol::Cell], width: u16) -> String {
    let mut lines = Vec::new();
    for row in cells.chunks(width.max(1) as usize) {
        let line: String = row.iter().map(|c| c.ch).collect();
        lines.push(line.trim_end().to_string());
    }
    lines.join("\n")
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let path = socket_path();
    let mut stream = UnixStream::connect(&path).await?;
    eprintln!("connected to {}", path.display());

    write_msg(
        &mut stream,
        &ClientMessage::Hello {
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
            capabilities: Capabilities::default(),
        },
    )
    .await?;

    let message = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Say hi in one word".to_string());

    let mut got_frame = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);

    loop {
        tokio::select! {
            res = read_msg(&mut stream) => {
                match res? {
                    ServerEvent::Welcome { .. } => {
                        eprintln!("welcome received; sending message: {}", message);
                        write_msg(&mut stream, &ClientMessage::PluginCommand {
                            plugin_id: "orbit-ai/nested-agent".to_string(),
                            name: "nested-agent.intercept".to_string(),
                            args_json: serde_json::json!({ "message": message }).to_string(),
                        }).await?;
                    }
                    ServerEvent::SurfaceFrame { plugin_id, width, cells, .. } => {
                        got_frame = true;
                        println!("=== frame from {} ({} cells) ===", plugin_id, cells.len());
                        println!("{}", frame_text(&cells, width));
                        println!();
                    }
                    ServerEvent::ProtocolError { code, message } => {
                        eprintln!("protocol error {}: {}", code, message);
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                eprintln!("timeout after 20s");
                break;
            }
        }
    }

    if !got_frame {
        eprintln!("NO SURFACE FRAME RECEIVED");
        std::process::exit(1);
    }
    Ok(())
}
