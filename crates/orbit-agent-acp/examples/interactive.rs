use orbit_agent_acp::{AcpClient, AcpClientConfig, AcpEvent};
use std::io::{self, BufRead, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("RUST_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    println!("=== Nested Agent ACP Test ===\n");

    println!("Spawning OpenCode ACP...");
    let mut client = AcpClient::spawn(AcpClientConfig {
        command: "opencode".to_string(),
        args: vec!["acp".to_string()],
        cwd: Some("/tmp".to_string()),
        env: vec![],
    })
    .await?;

    println!("[ok] OpenCode ACP spawned\n");

    let init_result = client.initialize().await?;
    println!("[ok] initialized");
    println!("     protocol: {}", init_result.protocol_version);
    println!(
        "     agent: {:?}",
        init_result.agent_capabilities.load_session
    );

    let session_id = client.new_session(None).await?;
    println!("\n[ok] session created: {}", session_id);

    println!("\n--- Interactive mode ---");
    println!("Type a message and press Enter. Commands:");
    println!("  /quit  - exit");
    println!("  /cancel - cancel current prompt");
    println!();

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        print!("> ");
        io::stdout().flush()?;

        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break,
        };

        if line.trim() == "/quit" {
            break;
        }

        if line.trim() == "/cancel" {
            client.cancel(&session_id).await?;
            println!("[cancelled]");
            continue;
        }

        if line.trim().is_empty() {
            continue;
        }

        println!("Sending: {}", line);
        match client.prompt(&session_id, &line).await {
            Ok(stop_reason) => println!("[prompt accepted, stop_reason: {}]", stop_reason),
            Err(e) => println!("[prompt error: {}]", e),
        }

        println!("Waiting for events...");
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(30), client.next_event())
                .await
            {
                Ok(Some(AcpEvent::SessionUpdate { update, .. })) => {
                    use orbit_agent_acp::SessionUpdate;
                    match update {
                        SessionUpdate::AgentMessageChunk(chunk) => {
                            if let orbit_agent_acp::ContentBlock::Text { text } = chunk.content {
                                print!("{}", text);
                                io::stdout().flush()?;
                            }
                        }
                        SessionUpdate::AgentMessage(msg) => {
                            if let Some(content) = msg.content {
                                for block in content {
                                    if let orbit_agent_acp::ContentBlock::Text { text } = block {
                                        print!("{}", text);
                                    }
                                }
                                io::stdout().flush()?;
                            }
                        }
                        SessionUpdate::ToolCall(tc) => {
                            println!("\n[tool: {}]", tc.title);
                        }
                        _ => {}
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    println!("\n[agent disconnected]");
                    break;
                }
                Err(_) => {
                    println!("\n[timeout waiting for events]");
                    break;
                }
            }
        }
        println!();
    }

    client.shutdown().await?;
    println!("\n[ok] shutdown complete");
    Ok(())
}
