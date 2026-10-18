#!/usr/bin/env bash
# One-shot setup for P1 plugin-runtime verification on a test machine.
# Idempotent: safe to re-run. Run from anywhere inside the orbt clone.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "==> fetch + checkout feature/plugin-p1"
git fetch origin
git checkout feature/plugin-p1
git pull --ff-only origin feature/plugin-p1

echo "==> build orbt (release)"
cargo build --release --offline 2>/dev/null || cargo build --release

echo "==> build nested-agent plugin (cdylib, release)"
cargo build --release --offline --manifest-path crates/orbit-plugin/examples/nested-agent/Cargo.toml 2>/dev/null \
  || cargo build --release --manifest-path crates/orbit-plugin/examples/nested-agent/Cargo.toml

PLUGIN_DIR=".orbit/modules/orbit-ai_nested-agent/1.0.0"
echo "==> install plugin to $PLUGIN_DIR"
mkdir -p "$PLUGIN_DIR"
cp crates/orbit-plugin/examples/nested-agent/target/release/libnested_agent.so "$PLUGIN_DIR/"

cat > "$PLUGIN_DIR/manifest.json" << 'EOF'
{
  "id": "orbit-ai/nested-agent",
  "version": "1.0.0",
  "runtime": "native",
  "entry": "./libnested_agent.so",
  "orbit_version": ">=0.4.0",
  "thread_affinity": "pinned",
  "contexts": ["session"],
  "surfaces": [
    {
      "kind": "plugin-dock",
      "id": "nested-agent-dock",
      "priority": 60,
      "min_size": { "w": 44, "h": 10 },
      "max_size": { "w": 44, "h": 20 }
    }
  ],
  "config_schema": {
    "type": "object",
    "properties": {
      "harness": { "type": "string", "enum": ["opencode", "claude-code"], "default": "opencode" },
      "model": { "type": "string", "default": "" },
      "max_history": { "type": "integer", "default": 100 },
      "auto_activate": { "type": "boolean", "default": true }
    }
  },
  "capabilities": [
    { "name": "runtime.native" },
    { "name": "render.plugin-dock" }
  ]
}
EOF

echo "==> write plugin config (auto_activate)"
mkdir -p "$HOME/.orbit/plugin-configs"
printf 'auto_activate = true\n' > "$HOME/.orbit/plugin-configs/orbit-ai_nested-agent.toml"

echo
echo "Setup complete. Next:"
echo "  1. ./target/release/orbt"
echo "  2. In the UI: Ctrl+B P (focus plugin dock), type a message, Enter"
echo "  3. Expect the OpenCode (Zen model) reply to stream into the dock."
