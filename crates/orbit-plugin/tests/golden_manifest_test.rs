use orbit_plugin::taxonomy::Taxonomy;
use orbit_plugin::manifest::Manifest;

fn parse_spec_example(json: &str) -> Manifest {
    Manifest::parse(json).expect("spec example should parse")
}

#[test]
fn taxonomy_embeds_and_parses() {
    let taxonomy = Taxonomy::embedded().expect("taxonomy should parse");
    assert!(taxonomy.is_valid_topic("pane.output"));
    assert!(taxonomy.is_valid_topic("pane.output.match"));
    assert!(taxonomy.is_valid_topic("agent.state"));
    assert!(taxonomy.is_valid_topic("window.layout-changed"));
    assert!(!taxonomy.is_valid_topic("nonexistent.topic"));
}

#[test]
fn nested_agent_manifest_valid() {
    let manifest_json = r#"{
        "id": "orbit-ai/nested-agent",
        "version": "1.0.0",
        "runtime": "wasm",
        "entry": "./nested-agent.wasm",
        "orbit_version": ">=0.4.0",
        "contexts": ["pane"],
        "surfaces": [
            {
                "kind": "pane-overlay",
                "id": "nested-ui",
                "priority": 80
            }
        ],
        "hooks": [
            {
                "topic": "pane.output.match"
            },
            {
                "topic": "agent.state"
            }
        ],
        "commands_used": ["pane.send-text", "pane.intervene"],
        "commands_provided": [
            {
                "name": "nested-agent.intercept",
                "description": "Intercept nested agent output"
            }
        ],
        "capabilities": [
            {
                "name": "perceive.pane-output-match",
                "scopes": [{"key": "phase", "value": "self"}]
            },
            {
                "name": "command.pane-write",
                "scopes": [{"key": "target", "value": "self"}]
            },
            {
                "name": "modal.show",
                "scopes": []
            }
        ]
    }"#;

    let manifest = parse_spec_example(manifest_json);
    assert_eq!(manifest.id.namespace(), "orbit-ai");
    assert_eq!(manifest.id.name(), "nested-agent");
    assert_eq!(manifest.runtime, orbit_plugin::manifest::Runtime::Wasm);
    assert_eq!(manifest.contexts.len(), 1);
    assert_eq!(manifest.surfaces.len(), 1);
    assert_eq!(manifest.hooks.len(), 2);
    assert_eq!(manifest.commands_used.len(), 2);
    assert_eq!(manifest.commands_provided.len(), 1);
    assert_eq!(manifest.capabilities.len(), 3);
}

#[test]
fn orchestrator_manifest_valid() {
    let manifest_json = r#"{
        "id": "orbit-ai/agent-orchestrator",
        "version": "2.1.0",
        "runtime": "wasm",
        "entry": "./orchestrator.wasm",
        "orbit_version": ">=0.4.0",
        "contexts": ["session"],
        "surfaces": [
            {
                "kind": "plugin-dock",
                "id": "orchestrator-dock",
                "priority": 60,
                "min_size": {"w": 200, "h": 20},
                "max_size": {"w": 400, "h": 60}
            },
            {
                "kind": "status-bar-slot",
                "id": "orchestrator-status",
                "priority": 50
            }
        ],
        "hooks": [
            {"topic": "agent.state"},
            {"topic": "agent.task-finished"},
            {"topic": "session.status"}
        ],
        "commands_used": ["agent.list", "agent.batch-dispatch", "agent.set-policy"],
        "commands_provided": [
            {
                "name": "orchestrator.batch-restart",
                "description": "Restart all errored agents"
            }
        ],
        "capabilities": [
            {"name": "perceive.agent-state", "scopes": []},
            {"name": "perceive.agent-task", "scopes": []},
            {"name": "command.agent-batch", "scopes": []},
            {"name": "render.plugin-dock", "scopes": []}
        ]
    }"#;

    let manifest = parse_spec_example(manifest_json);
    assert_eq!(manifest.id.namespace(), "orbit-ai");
    assert_eq!(manifest.id.name(), "agent-orchestrator");
    assert_eq!(manifest.contexts.len(), 1);
    assert_eq!(manifest.surfaces.len(), 2);
    assert_eq!(manifest.hooks.len(), 3);
    assert_eq!(manifest.commands_used.len(), 3);
    assert_eq!(manifest.capabilities.len(), 4);
}

#[test]
fn agent_monitor_manifest_valid() {
    let manifest_json = r#"{
        "id": "orbit-core/agent-monitor",
        "version": "1.0.0",
        "runtime": "native",
        "entry": "./agent-monitor.so",
        "orbit_version": ">=0.4.0",
        "contexts": ["session"],
        "thread_affinity": "pinned",
        "surfaces": [
            {
                "kind": "agent-monitor-dock",
                "id": "agent-monitor",
                "priority": 90,
                "min_size": {"w": 180, "h": 12},
                "max_size": {"w": 400, "h": 60}
            }
        ],
        "hooks": [
            {"topic": "agent.state"},
            {"topic": "agent.detected"},
            {"topic": "agent.task-finished"}
        ],
        "commands_used": ["agent.list"],
        "commands_provided": [],
        "capabilities": [
            {"name": "runtime.native", "scopes": []},
            {"name": "perceive.agent-state", "scopes": []},
            {"name": "render.agent-monitor-dock", "scopes": []}
        ]
    }"#;

    let manifest = parse_spec_example(manifest_json);
    assert_eq!(manifest.runtime, orbit_plugin::manifest::Runtime::Native);
    assert_eq!(
        manifest.thread_affinity,
        orbit_plugin::manifest::ThreadAffinity::Pinned
    );
    assert_eq!(manifest.surfaces.len(), 1);
    assert_eq!(manifest.commands_provided.len(), 0);
}

#[test]
fn invalid_manifest_missing_required() {
    let result = Manifest::parse(r#"{"id": "test/plugin"}"#);
    assert!(result.is_err());
}

#[test]
fn invalid_manifest_bad_version() {
    let result = Manifest::parse(
        r#"{
        "id": "test/plugin",
        "version": "not-semver",
        "runtime": "wasm",
        "entry": "./test.wasm",
        "orbit_version": ">=0.4.0"
    }"#,
    );
    assert!(result.is_err());
}
