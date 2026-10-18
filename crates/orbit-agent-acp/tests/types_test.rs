use orbit_agent_acp::*;

#[test]
fn acp_types_serialization() {
    let init_params = InitializeParams {
        protocol_version: ACP_PROTOCOL_VERSION,
        client_capabilities: ClientCapabilities {
            fs: Some(FsCapabilities {
                read_text_file: true,
                write_text_file: true,
            }),
            terminal: Some(true),
        },
    };

    let json = serde_json::to_value(&init_params).unwrap();
    assert_eq!(json["protocolVersion"], 1);
}

#[test]
fn acp_content_block_text() {
    let block = ContentBlock::Text {
        text: "Hello".to_string(),
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "text");
    assert_eq!(json["text"], "Hello");
}

#[test]
fn acp_session_update_agent_message_chunk() {
    let update = SessionUpdate::AgentMessageChunk(ContentChunk {
        content: ContentBlock::Text {
            text: "chunk".to_string(),
        },
        message_id: Some("msg-1".to_string()),
    });

    let json = serde_json::to_value(&update).unwrap();
    assert_eq!(json["sessionUpdate"], "agent_message_chunk");
    assert_eq!(json["content"]["text"], "chunk");
}

#[test]
fn acp_prompt_params() {
    let params = PromptParams {
        session_id: "sess-1".to_string(),
        prompt: vec![ContentBlock::Text {
            text: "test prompt".to_string(),
        }],
    };

    let json = serde_json::to_value(&params).unwrap();
    assert_eq!(json["sessionId"], "sess-1");
    assert_eq!(json["prompt"][0]["type"], "text");
}

#[test]
fn acp_jsonrpc_roundtrip() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {}
        }
    });

    let parsed: serde_json::Value = serde_json::from_str(&request.to_string()).unwrap();
    assert_eq!(parsed["jsonrpc"], "2.0");
    assert_eq!(parsed["id"], 1);
    assert_eq!(parsed["method"], "initialize");
}
