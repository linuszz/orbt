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
