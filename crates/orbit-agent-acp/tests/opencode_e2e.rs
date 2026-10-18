use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn send_jsonrpc(stdin: &mut impl Write, id: u64, method: &str, params: serde_json::Value) {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    });
    let mut line = msg.to_string();
    line.push('\n');
    stdin.write_all(line.as_bytes()).unwrap();
    stdin.flush().unwrap();
}

#[test]
fn test_opencode_acp_initialize() {
    let mut child = Command::new("opencode")
        .args(["acp", "--pure"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn opencode acp");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    send_jsonrpc(
        &mut stdin,
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": {
                    "readTextFile": true,
                    "writeTextFile": true
                },
                "terminal": true
            }
        }),
    );

    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut got_response = false;
    for line in lines.by_ref().take(10) {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&line) {
            if resp.get("id").and_then(|i| i.as_u64()) == Some(1) {
                let result = &resp["result"];
                assert!(result.is_object(), "expected result object, got: {}", result);

                let protocol_version = result["protocolVersion"].as_u64();
                assert_eq!(protocol_version, Some(1), "protocol version mismatch");

                println!("Initialize response: {}", result);
                got_response = true;
                break;
            }
        }
    }

    assert!(got_response, "did not receive initialize response");
    let _ = child.kill();
}

#[test]
fn test_opencode_acp_session_new() {
    let mut child = Command::new("opencode")
        .args(["acp", "--pure"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn opencode acp");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    send_jsonrpc(
        &mut stdin,
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": 1,
            "clientCapabilities": {}
        }),
    );

    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    for line in lines.by_ref() {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&line) {
            if resp.get("id").and_then(|i| i.as_u64()) == Some(1) {
                break;
            }
        }
    }

    send_jsonrpc(
        &mut stdin,
        2,
        "session/new",
        serde_json::json!({
            "cwd": "/tmp",
            "mcpServers": []
        }),
    );

    let mut got_session = false;
    for line in lines.by_ref().take(10) {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&line) {
            if resp.get("id").and_then(|i| i.as_u64()) == Some(2) {
                println!("Session/new response: {}", resp);
                let result = &resp["result"];
                let session_id = result["sessionId"].as_str();
                assert!(session_id.is_some(), "expected sessionId in response");
                println!("Session created: {}", session_id.unwrap());
                got_session = true;
                break;
            }
        }
    }

    assert!(got_session, "did not receive session/new response");
    let _ = child.kill();
}

#[test]
fn test_opencode_acp_prompt_simple() {
    let mut child = Command::new("opencode")
        .args(["acp", "--pure"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn opencode acp");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    send_jsonrpc(
        &mut stdin,
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": 1,
            "clientCapabilities": {}
        }),
    );

    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    for line in lines.by_ref() {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&line) {
            if resp.get("id").and_then(|i| i.as_u64()) == Some(1) {
                break;
            }
        }
    }

    send_jsonrpc(
        &mut stdin,
        2,
        "session/new",
        serde_json::json!({
            "cwd": "/tmp",
            "mcpServers": []
        }),
    );

    let mut session_id = String::new();
    for line in lines.by_ref().take(10) {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&line) {
            if resp.get("id").and_then(|i| i.as_u64()) == Some(2) {
                session_id = resp["result"]["sessionId"].as_str().unwrap().to_string();
                break;
            }
        }
    }

    assert!(!session_id.is_empty(), "failed to create session");

    send_jsonrpc(
        &mut stdin,
        3,
        "session/prompt",
        serde_json::json!({
            "sessionId": session_id,
            "prompt": [{
                "type": "text",
                "text": "Hi"
            }]
        }),
    );

    let mut got_response = false;
    let mut got_agent_text = false;
    let mut agent_text = String::new();

    for line in lines.by_ref().take(200) {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&line) {
            if resp.get("id").and_then(|i| i.as_u64()) == Some(3) {
                println!("Prompt response: {}", resp);
                got_response = true;
            }
        }

        if let Ok(notif) = serde_json::from_str::<serde_json::Value>(&line) {
            if notif.get("method").and_then(|m| m.as_str()) == Some("session/update") {
                let update = &notif["params"]["update"];
                let update_type = update["sessionUpdate"].as_str().unwrap_or("");

                match update_type {
                    "agent_message_chunk" => {
                        if let Some(text) = update["content"]["text"].as_str() {
                            agent_text.push_str(text);
                            print!("{}", text);
                            got_agent_text = true;
                        }
                    }
                    "agent_message" => {
                        if let Some(content) = update["content"].as_array() {
                            for block in content {
                                if let Some(text) = block["text"].as_str() {
                                    agent_text.push_str(text);
                                    got_agent_text = true;
                                }
                            }
                        }
                    }
                    "tool_call" => {
                        let title = update["title"].as_str().unwrap_or("unknown");
                        println!("\n[tool] {}", title);
                    }
                    _ => {}
                }
            }
        }

        if got_response && got_agent_text {
            break;
        }
    }

    println!("\nAgent text: {}", agent_text);
    assert!(got_response, "did not receive prompt response");
    assert!(got_agent_text, "did not receive any agent text");

    drop(stdin);
    let _ = child.wait();
}
