//! Agent runtime: detection, state machine, registry.
//! See `06_tech-design/04-server-architecture.md` §4 and `07-agent-data-model.md`.

use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use orbt_protocol::{AgentDetail, AgentMetrics};
use orbt_protocol::{AgentId, AgentInfo, AgentProtocol, AgentStatus, PaneId, ServerEvent, SpaceId};
use tokio::sync::{broadcast, RwLock};
use tracing::debug;

/// CLI process names that identify known agent tools.
#[cfg(target_os = "linux")]
const AGENT_PATTERNS: &[&str] = &[
    "claude",
    "claude-code",
    "opencode",
    "codex",
    "aider",
    "gh-copilot",
    "cursor",
];

/// Script runner executables — when detected, also scan their path arguments.
#[cfg(target_os = "linux")]
const SCRIPT_RUNNERS: &[&str] = &["node", "npx", "python", "python3", "deno", "bun"];

/// Agent names that support the Agent Client Protocol (ACP).
/// When detected, these are marked AcpCapable rather than Heuristic.
#[cfg(target_os = "linux")]
const ACP_CAPABLE_AGENTS: &[&str] = &[
    "opencode",
    "gemini",
    "gemini-cli",
    "codex",
    "codex-cli",
    "cline",
];

/// Output patterns that indicate an agent is waiting for user input (Blocked/Eclipse state).
const BLOCK_PATTERNS: &[&str] = &[
    // Standard confirmation prompts
    "(y/n)",
    "(Y/N)",
    "(yes/no)",
    "[y/n]",
    "[Y/n]",
    "[n/Y]",
    "[Y/N]",
    "Do you want",
    "Are you sure",
    "Press Enter",
    "press any key",
    "Continue?",
    "Proceed?",
    // Claude Code specific patterns
    "Do you want to proceed",
    "Allow this action",
    "Allow Claude to",
    "approve this",
    "Approve this",
    "confirm this",
    "Confirm this",
    // opencode patterns
    "Do you want to run",
    "Allow tool",
    "Approve tool",
    // Aider patterns
    "Add these files",
    "Create new file",
    "Apply edits",
    // Generic shell/readline prompts that need input
    "Enter your",
    "Enter the",
    "Type your",
    "Password:",
    "password:",
    "Passphrase:",
    "passphrase:",
    // npm/yarn interactive
    "package name:",
    "version:",
    "description:",
];

pub struct AgentRegistry {
    agents: RwLock<Vec<AgentInfo>>,
    /// Maps AgentId → OS PID for abort support.
    pid_map: RwLock<HashMap<AgentId, u32>>,
    #[cfg(target_os = "linux")]
    next_id: AtomicU32,
    event_bus: broadcast::Sender<ServerEvent>,
}

impl AgentRegistry {
    pub fn new(event_bus: broadcast::Sender<ServerEvent>) -> Arc<Self> {
        Arc::new(Self {
            agents: RwLock::new(Vec::new()),
            pid_map: RwLock::new(HashMap::new()),
            #[cfg(target_os = "linux")]
            next_id: AtomicU32::new(0),
            event_bus,
        })
    }

    pub async fn get_agents(&self) -> Vec<AgentInfo> {
        self.agents.read().await.clone()
    }

    /// Remove an agent from the visible list immediately (user-driven dismiss).
    pub async fn remove_agent(&self, agent_id: AgentId) {
        self.agents.write().await.retain(|a| a.id != agent_id);
        self.pid_map.write().await.remove(&agent_id);
        let _ = self.event_bus.send(ServerEvent::AgentRemoved(agent_id));
        debug!("agent removed by user: id={agent_id:?}");
    }

    /// Clear an Error-state agent back to Idle so it can be re-detected.
    pub async fn restart_agent(&self, agent_id: AgentId) {
        let mut agents = self.agents.write().await;
        if let Some(agent) = agents.iter_mut().find(|a| a.id == agent_id) {
            agent.status = AgentStatus::Idle;
            agent.detail = None;
            let _ = self.event_bus.send(ServerEvent::AgentStatusChanged {
                agent_id,
                new_status: AgentStatus::Idle,
                detail: None,
            });
            debug!("agent restarted (reset to Idle): id={agent_id:?}");
        }
    }

    /// Kill the agent process with SIGTERM.
    pub async fn abort_agent(&self, agent_id: AgentId) {
        let pid = {
            let map = self.pid_map.read().await;
            map.get(&agent_id).copied()
        };
        if let Some(pid) = pid {
            #[cfg(unix)]
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            debug!("sent SIGTERM to agent {agent_id:?} pid={pid}");
        }
    }

    /// Start a background task that polls for agent child processes of `shell_pid`
    /// and scans PTY output for block patterns (Satellite Eclipse detection).
    /// On non-Linux, process detection is skipped (no /proc); block scanning still runs.
    pub fn watch_pane(self: Arc<Self>, pane_id: PaneId, space_id: SpaceId, shell_pid: u32) {
        tokio::spawn(async move {
            // On non-Linux, process scanning is unavailable.
            #[cfg(not(target_os = "linux"))]
            let _ = (space_id, shell_pid);

            // pid → (AgentId, start_time); only populated on Linux
            #[cfg(target_os = "linux")]
            let mut tracked: HashMap<u32, (AgentId, Instant)> = HashMap::new();
            #[cfg(not(target_os = "linux"))]
            let tracked: HashMap<u32, (AgentId, Instant)> = HashMap::new();

            // Last meaningful output line seen (shown in card row 2); Linux only.
            #[cfg(target_os = "linux")]
            let mut last_output_line = String::new();

            // Last progress percentage detected; Linux only.
            #[cfg(target_os = "linux")]
            let mut last_progress_pct: Option<f32> = None;

            // pid → cpu ticks at last 5-second sample; Linux only.
            #[cfg(target_os = "linux")]
            let mut cpu_prev: HashMap<u32, u64> = HashMap::new();
            #[cfg(target_os = "linux")]
            let mut cpu_prev_time = Instant::now();

            let mut poll_count: u32 = 0;
            let mut rx = self.event_bus.subscribe();
            // Start first tick after 500 ms to match old sleep-at-top-of-loop behaviour.
            let tick_start = tokio::time::Instant::now() + Duration::from_millis(500);
            let mut interval = tokio::time::interval_at(tick_start, Duration::from_millis(500));

            loop {
                tokio::select! {
                    // --- PTY output: scan for Eclipse block patterns ---
                    event = rx.recv() => {
                        match event {
                            Ok(ServerEvent::PaneOutput { pane_id: ev_pane, data })
                                if ev_pane == pane_id =>
                            {
                                // Check last 256 bytes for prompt patterns to avoid
                                // matching historical content in a large chunk.
                                let tail_start = data.len().saturating_sub(256);
                                let tail = String::from_utf8_lossy(&data[tail_start..]);

                                // Update last meaningful output line (Linux only).
                                #[cfg(target_os = "linux")]
                                if let Some(line) = tail
                                    .lines()
                                    .map(|l| l.trim())
                                    .rfind(|l| !l.is_empty() && l.len() > 3)
                                {
                                    last_output_line = strip_ansi(line);
                                }

                                // Block-pattern scan only meaningful when agents are tracked.
                                if tracked.is_empty() {
                                    continue;
                                }

                                let is_prompt =
                                    BLOCK_PATTERNS.iter().any(|p| tail.contains(p));

                                // Scan for progress percentage ("75%" or "75.5%").
                                #[cfg(target_os = "linux")]
                                if let Some(pct) = extract_progress(&tail) {
                                    last_progress_pct = Some(pct);
                                }

                                for (agent_id, _) in tracked.values() {
                                    let current = {
                                        let agents = self.agents.read().await;
                                        agents
                                            .iter()
                                            .find(|a| a.id == *agent_id)
                                            .map(|a| a.status.clone())
                                    };
                                    match current {
                                        Some(AgentStatus::Working) if is_prompt => {
                                            let block_msg = strip_ansi(
                                                tail.lines()
                                                    .rfind(|l| !l.trim().is_empty())
                                                    .unwrap_or(""),
                                            );
                                            let new_detail = {
                                                let mut agents = self.agents.write().await;
                                                agents
                                                    .iter_mut()
                                                    .find(|a| a.id == *agent_id)
                                                    .filter(|a| {
                                                        a.status == AgentStatus::Working
                                                    })
                                                    .map(|a| {
                                                        a.status = AgentStatus::Blocked;
                                                        let d = a
                                                            .detail
                                                            .get_or_insert_with(Default::default);
                                                        d.block_msg = Some(block_msg);
                                                        a.detail.clone().unwrap()
                                                    })
                                            };
                                            if let Some(detail) = new_detail {
                                                let _ = self.event_bus.send(
                                                    ServerEvent::AgentStatusChanged {
                                                        agent_id: *agent_id,
                                                        new_status: AgentStatus::Blocked,
                                                        detail: Some(detail),
                                                    },
                                                );
                                                debug!("agent blocked: id={agent_id:?}");
                                            }
                                        }
                                        Some(AgentStatus::Blocked) if !is_prompt => {
                                            let new_detail = {
                                                let mut agents = self.agents.write().await;
                                                agents
                                                    .iter_mut()
                                                    .find(|a| a.id == *agent_id)
                                                    .filter(|a| {
                                                        a.status == AgentStatus::Blocked
                                                    })
                                                    .map(|a| {
                                                        a.status = AgentStatus::Working;
                                                        let d = a
                                                            .detail
                                                            .get_or_insert_with(Default::default);
                                                        d.block_msg = None;
                                                        a.detail.clone().unwrap()
                                                    })
                                            };
                                            if let Some(detail) = new_detail {
                                                let _ = self.event_bus.send(
                                                    ServerEvent::AgentStatusChanged {
                                                        agent_id: *agent_id,
                                                        new_status: AgentStatus::Working,
                                                        detail: Some(detail),
                                                    },
                                                );
                                                debug!("agent unblocked: id={agent_id:?}");
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(_) => break,
                        }
                    }

                    // --- 500 ms poll: /proc child detection + duration ticks ---
                    _ = interval.tick() => {
                        poll_count += 1;

                        #[cfg(target_os = "linux")]
                        if !process_exists(shell_pid) {
                            break;
                        }

                        #[cfg(target_os = "linux")]
                        {
                            let children = child_processes(shell_pid);
                            let child_set: HashSet<u32> = children.iter().copied().collect();

                            // Detect newly appeared agents.
                            for cpid in &children {
                                if tracked.contains_key(cpid) {
                                    continue;
                                }
                                if let Some(name) = agent_name_for(*cpid) {
                                    let id =
                                        AgentId(self.next_id.fetch_add(1, Ordering::Relaxed));
                                    let model = extract_model(*cpid).unwrap_or_default();
                                    let task = extract_task(*cpid);
                                    let protocol = if ACP_CAPABLE_AGENTS.contains(&name.as_str()) {
                                        AgentProtocol::AcpCapable
                                    } else {
                                        AgentProtocol::Heuristic
                                    };
                                    let info = AgentInfo {
                                        id,
                                        name,
                                        space_id,
                                        pane_id: Some(pane_id),
                                        model,
                                        status: AgentStatus::Working,
                                        detail: Some(AgentDetail {
                                            task,
                                            ..Default::default()
                                        }),
                                        protocol,
                                    };
                                    tracked.insert(*cpid, (id, Instant::now()));
                                    self.agents.write().await.push(info.clone());
                                    self.pid_map.write().await.insert(id, *cpid);
                                    let _ = self.event_bus.send(ServerEvent::AgentCreated(info));
                                    debug!("agent detected: pid={cpid} id={id:?}");
                                }
                            }

                            // Detect agents that have exited.
                            let exited: Vec<(u32, AgentId)> = tracked
                                .iter()
                                .filter(|(pid, _)| !child_set.contains(pid))
                                .map(|(&pid, &(id, _))| (pid, id))
                                .collect();

                            for (cpid, agent_id) in exited {
                                tracked.remove(&cpid);
                                {
                                    let mut agents = self.agents.write().await;
                                    if let Some(a) =
                                        agents.iter_mut().find(|a| a.id == agent_id)
                                    {
                                        a.status = AgentStatus::Done;
                                    }
                                }
                                self.pid_map.write().await.remove(&agent_id);
                                let _ = self.event_bus.send(ServerEvent::AgentStatusChanged {
                                    agent_id,
                                    new_status: AgentStatus::Done,
                                    detail: None,
                                });
                                debug!("agent done: pid={cpid} id={agent_id:?}");

                                let registry = Arc::clone(&self);
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_secs(30)).await;
                                    registry
                                        .agents
                                        .write()
                                        .await
                                        .retain(|a| a.id != agent_id);
                                    let _ = registry
                                        .event_bus
                                        .send(ServerEvent::AgentRemoved(agent_id));
                                });
                            }
                        }

                        // Every 10 polls (~5 s): emit duration updates and resource metrics.
                        // On non-Linux, tracked is always empty so this block is a no-op.
                        if poll_count % 10 == 0 {
                            #[cfg(target_os = "linux")]
                            {
                                let elapsed_secs = cpu_prev_time.elapsed().as_secs_f32();

                                for (cpid, (agent_id, start)) in &tracked {
                                    let duration_s = start.elapsed().as_secs() as u32;
                                    let (current_status, current_detail) = {
                                        let mut agents = self.agents.write().await;
                                        let Some(a) =
                                            agents.iter_mut().find(|a| a.id == *agent_id)
                                        else {
                                            continue;
                                        };
                                        let d = a.detail.get_or_insert_with(Default::default);
                                        d.duration_s = duration_s;
                                        if last_progress_pct.is_some() {
                                            d.progress = last_progress_pct;
                                        }
                                        (a.status.clone(), a.detail.clone().unwrap())
                                    };
                                    let _ = self.event_bus.send(ServerEvent::AgentStatusChanged {
                                        agent_id: *agent_id,
                                        new_status: current_status,
                                        detail: Some(current_detail),
                                    });

                                    let rss_kb = read_rss_kb(*cpid);
                                    let current_ticks = read_cpu_ticks(*cpid).unwrap_or(0);
                                    // clk_tck = 100 ticks/s on virtually all Linux systems.
                                    const CLK_TCK: f32 = 100.0;
                                    let cpu_percent = if elapsed_secs > 0.5 {
                                        let prev =
                                            cpu_prev.get(cpid).copied().unwrap_or(current_ticks);
                                        let delta = current_ticks.saturating_sub(prev) as f32;
                                        Some((delta / (elapsed_secs * CLK_TCK) * 100.0).min(100.0))
                                    } else {
                                        None
                                    };
                                    cpu_prev.insert(*cpid, current_ticks);
                                    let recent = if last_output_line.is_empty() {
                                        vec![]
                                    } else {
                                        vec![last_output_line.clone()]
                                    };
                                    let _ =
                                        self.event_bus.send(ServerEvent::AgentMetricsUpdated {
                                            agent_id: *agent_id,
                                            metrics: AgentMetrics {
                                                cpu_percent,
                                                rss_kb,
                                                recent_lines: recent,
                                            },
                                        });
                                }

                                cpu_prev_time = Instant::now();
                            }
                        }
                    }
                }
            }

            // Shell exited (or event bus closed) — mark remaining tracked agents done.
            for (cpid, (agent_id, _)) in &tracked {
                {
                    let mut agents = self.agents.write().await;
                    if let Some(a) = agents.iter_mut().find(|a| a.id == *agent_id) {
                        a.status = AgentStatus::Done;
                    }
                }
                self.pid_map.write().await.remove(agent_id);
                let _ = self.event_bus.send(ServerEvent::AgentStatusChanged {
                    agent_id: *agent_id,
                    new_status: AgentStatus::Done,
                    detail: None,
                });
                debug!("agent shell exited: pid={cpid}");
            }
        });
    }
}

/// Returns the canonical agent name for a PID if it is a known agent process.
#[cfg(target_os = "linux")]
fn agent_name_for(pid: u32) -> Option<String> {
    let cmdline = read_cmdline(pid)?;
    let args: Vec<&str> = cmdline.split('\0').filter(|s| !s.is_empty()).collect();
    agent_name_from_args(&args)
}

/// Pure matching logic: returns the canonical agent name given a cmdline argv slice.
/// `args[0]` is argv[0] (the executable name as the OS saw it, preserved through Nix wrappers).
#[cfg(target_os = "linux")]
fn agent_name_from_args(args: &[&str]) -> Option<String> {
    let exe = args.first().copied().unwrap_or("");
    let basename = std::path::Path::new(exe)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(exe);

    // Direct match on executable basename.
    for &pattern in AGENT_PATTERNS {
        if basename == pattern || basename.starts_with(&format!("{pattern}-")) {
            return Some(pattern.to_string());
        }
    }

    // For script runners, scan subsequent args for agent path components.
    if SCRIPT_RUNNERS.contains(&basename) {
        for arg in args.iter().skip(1) {
            if arg.starts_with('-') {
                continue;
            }
            for component in std::path::Path::new(arg).components() {
                if let std::path::Component::Normal(name) = component {
                    if let Some(s) = name.to_str() {
                        for &pattern in AGENT_PATTERNS {
                            if s == pattern || s.starts_with(&format!("{pattern}-")) {
                                return Some(pattern.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Extract the `--model` value from a process cmdline, if present.
/// Also reads OPENCODE_MODEL from process environment for opencode.
#[cfg(target_os = "linux")]
fn extract_model(pid: u32) -> Option<String> {
    let cmdline = read_cmdline(pid)?;
    let args: Vec<&str> = cmdline.split('\0').filter(|s| !s.is_empty()).collect();

    // Check cmdline flags first (works for all agents including opencode).
    for (i, arg) in args.iter().enumerate() {
        if *arg == "--model" || *arg == "-m" {
            return args.get(i + 1).map(|s| s.to_string());
        }
        if let Some(model) = arg.strip_prefix("--model=") {
            return Some(model.to_string());
        }
    }

    // opencode falls back to OPENCODE_MODEL env var.
    let exe = args.first().copied().unwrap_or("");
    let basename = std::path::Path::new(exe)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if basename == "opencode" || basename.starts_with("opencode-") {
        if let Ok(env) = std::fs::read_to_string(format!("/proc/{pid}/environ")) {
            for entry in env.split('\0') {
                if let Some(val) = entry.strip_prefix("OPENCODE_MODEL=") {
                    return Some(val.to_string());
                }
            }
        }
    }

    None
}

/// Extract a short task description from positional (non-flag) cmdline args.
#[cfg(target_os = "linux")]
fn extract_task(pid: u32) -> Option<String> {
    let cmdline = read_cmdline(pid)?;
    let args: Vec<&str> = cmdline.split('\0').filter(|s| !s.is_empty()).collect();
    let positional: Vec<&str> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .copied()
        .collect();
    if positional.is_empty() {
        return None;
    }
    Some(
        positional
            .iter()
            .take(2)
            .copied()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[cfg(target_os = "linux")]
fn read_cmdline(pid: u32) -> Option<String> {
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Read the resident set size (VmRSS) for a process from /proc/<pid>/status.
#[cfg(target_os = "linux")]
fn read_rss_kb(pid: u32) -> Option<u32> {
    let content = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok());
        }
    }
    None
}

/// Read total CPU ticks (utime+stime) for a process from /proc/<pid>/stat.
/// The `comm` field is in parentheses and may contain spaces, so we find the
/// closing `)` and index fields relative to it.
#[cfg(target_os = "linux")]
fn read_cpu_ticks(pid: u32) -> Option<u64> {
    let content = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Skip past closing ')' of comm field; remaining fields start with state.
    let rest = content[content.find(')')?.checked_add(1)?..].trim_start();
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After ')': state(0) ppid(1) pgrp(2) session(3) tty_nr(4) tpgid(5) flags(6)
    // minflt(7) cminflt(8) majflt(9) cmajflt(10) utime(11) stime(12)
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// Returns the immediate child PIDs of `parent_pid`.
#[cfg(target_os = "linux")]
fn child_processes(parent_pid: u32) -> Vec<u32> {
    // Fast path: kernel exposes direct children in /proc/<pid>/task/<pid>/children.
    let children_path = format!("/proc/{parent_pid}/task/{parent_pid}/children");
    if let Ok(content) = std::fs::read_to_string(&children_path) {
        let pids: Vec<u32> = content
            .split_whitespace()
            .filter_map(|s| s.parse::<u32>().ok())
            .collect();
        if !pids.is_empty() || content.trim().is_empty() {
            return pids;
        }
    }

    // Fallback: scan every /proc/<n>/status for PPid matching parent_pid.
    let mut result = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return result;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let Ok(pid) = name_str.parse::<u32>() else {
            continue;
        };
        if pid == parent_pid {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
            continue;
        };
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("PPid:") {
                if let Ok(ppid) = rest.trim().parse::<u32>() {
                    if ppid == parent_pid {
                        result.push(pid);
                    }
                }
                break;
            }
        }
    }
    result
}

#[cfg(target_os = "linux")]
fn process_exists(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Strip ANSI/VT escape sequences (CSI, OSC, ESC+single-char) from `text`.
/// Used to clean up PTY output before storing it as block_msg or last_output_line.
fn strip_ansi(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i >= bytes.len() {
                break;
            }
            match bytes[i] {
                b'[' => {
                    // CSI: ESC [ ... final (final is 0x40–0x7e)
                    i += 1;
                    while i < bytes.len() && !(0x40..=0x7eu8).contains(&bytes[i]) {
                        i += 1;
                    }
                    i += 1; // skip final byte
                }
                b']' => {
                    // OSC: ESC ] ... ST (ST = ESC\ or BEL)
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    // ESC + single char (e.g. ESC M, ESC 7, ESC 8)
                    i += 1;
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Scan `text` for the last "N%" or "N.N%" pattern and return it as a fraction [0.0, 1.0].
/// Only matches values in [0.0, 100.0]. Returns None if none found.
#[cfg(target_os = "linux")]
fn extract_progress(text: &str) -> Option<f32> {
    let bytes = text.as_bytes();
    let mut last: Option<f32> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'%' {
                if let Ok(pct) = text[start..i].parse::<f32>() {
                    if (0.0..=100.0).contains(&pct) {
                        last = Some(pct / 100.0);
                    }
                }
                i += 1;
                continue;
            }
        } else {
            i += 1;
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn extract_progress_basic() {
        assert_eq!(extract_progress("Compiling 75%"), Some(0.75));
        assert_eq!(extract_progress("done 100%"), Some(1.0));
        assert_eq!(extract_progress("0%"), Some(0.0));
        assert_eq!(extract_progress("no percent here"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn extract_progress_last_wins() {
        assert_eq!(extract_progress("step 1: 30% step 2: 60%"), Some(0.6));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn extract_progress_decimal() {
        assert_eq!(extract_progress("99.5%"), Some(0.995));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn extract_progress_out_of_range_ignored() {
        assert_eq!(extract_progress("200% error"), None);
        assert_eq!(extract_progress("101%"), None);
    }

    #[test]
    fn strip_ansi_basic() {
        assert_eq!(strip_ansi("hello"), "hello");
        assert_eq!(strip_ansi("\x1b[32mgreen\x1b[0m"), "green");
        assert_eq!(strip_ansi("\x1b]0;title\x07text"), "text");
    }

    #[test]
    fn strip_ansi_csi_sequences() {
        // CSI cursor move + reset
        assert_eq!(strip_ansi("\x1b[2J\x1b[H"), "");
        assert_eq!(strip_ansi("a\x1b[1mb\x1b[0mc"), "abc");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detect_claude_native_binary() {
        // claude binary (non-Nix): argv[0] = "claude"
        let args = &["claude", "--print", "say hi"];
        assert_eq!(agent_name_from_args(args), Some("claude".to_string()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detect_claude_nix_wrapper() {
        // Nix wraps claude: argv[0] is preserved as "claude" even though
        // /proc/pid/exe points to .claude-wrapped
        let args = &["claude", "--model", "claude-opus-4-5", "--print", "say hi"];
        assert_eq!(agent_name_from_args(args), Some("claude".to_string()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detect_opencode_via_node_pnpm() {
        // opencode bash wrapper: exec pnpm dlx opencode-ai@latest
        // pnpm itself is node, so argv[0] = node and "opencode" appears as a positional arg
        let args = &[
            "/nix/store/hash-nodejs/bin/node",
            "/nix/store/hash-pnpm/bin/pnpm",
            "dlx",
            "opencode",
        ];
        assert_eq!(agent_name_from_args(args), Some("opencode".to_string()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detect_opencode_via_node_pnpm_versioned() {
        // pnpm dlx with version suffix: "opencode-ai@latest" path component
        let args = &[
            "/nix/store/hash-nodejs/bin/node",
            "/nix/store/hash-pnpm/bin/pnpm",
            "dlx",
            "opencode-ai@latest",
        ];
        assert_eq!(agent_name_from_args(args), Some("opencode".to_string()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detect_opencode_via_node_script_path() {
        // Node running the downloaded opencode package directly
        let args = &[
            "/nix/store/hash-nodejs/bin/node",
            "/home/user/.cache/pnpm/dlx/hash/node_modules/opencode/dist/index.js",
        ];
        assert_eq!(agent_name_from_args(args), Some("opencode".to_string()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detect_unrelated_node_process_not_matched() {
        let args = &["/usr/bin/node", "/home/user/.local/bin/some-other-tool"];
        assert_eq!(agent_name_from_args(args), None);
    }
}
