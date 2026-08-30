#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use piko_comms::{ThreadBridgeReceiver, contracts::TuiHostBridge, thread_bridge};
use piko_protocol::{Command as HostCommand, CommandResult, ServerMessage, SessionSnapshot};
use serde_json::Value;

pub static E2E_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
    E2E_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

enum HostLine {
    Message(Box<ServerMessage>),
    DecodeError(String),
    Closed,
}

pub struct HostdHarness {
    child: Child,
    stdin: ChildStdin,
    rx: ThreadBridgeReceiver<TuiHostBridge, HostLine>,
    backlog: Vec<ServerMessage>,
    root: tempfile::TempDir,
    mode: String,
    release_path: PathBuf,
    gateway_log_path: PathBuf,
    latest_submitted: HashMap<String, String>,
    input_sessions: HashMap<String, String>,
    active_work: HashMap<(String, String), String>,
    completed_work: HashSet<String>,
    terminal_usage: HashSet<String>,
    terminal_reconciled: HashSet<String>,
}

impl HostdHarness {
    pub fn launch(mode: &str) -> Self {
        let root = tempfile::tempdir().expect("create e2e root");
        let cwd = root.path().join("workspace");
        let session_dir = root.path().join("sessions");
        let piko_home = root.path().join("piko-home");
        std::fs::create_dir_all(&cwd).expect("create e2e workspace");
        std::fs::create_dir_all(&session_dir).expect("create e2e session root");
        std::fs::create_dir_all(&piko_home).expect("create e2e piko home");
        let release_path = root.path().join("release");
        let trace_path = root.path().join("trace.jsonl");
        let gateway_log_path = root.path().join("gateway.jsonl");
        let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("canonicalize source root");
        let (child, stdin, rx) = Self::spawn_process(
            mode,
            &root,
            &release_path,
            &trace_path,
            &gateway_log_path,
            &source_root,
            &session_dir,
            &piko_home,
        );

        Self {
            child,
            stdin,
            rx,
            backlog: Vec::new(),
            root,
            mode: mode.into(),
            release_path,
            gateway_log_path,
            latest_submitted: HashMap::new(),
            input_sessions: HashMap::new(),
            active_work: HashMap::new(),
            completed_work: HashSet::new(),
            terminal_usage: HashSet::new(),
            terminal_reconciled: HashSet::new(),
        }
    }

    #[allow(clippy::disallowed_methods, clippy::too_many_arguments)]
    fn spawn_process(
        mode: &str,
        root: &tempfile::TempDir,
        release_path: &Path,
        trace_path: &Path,
        gateway_log_path: &Path,
        source_root: &Path,
        session_dir: &Path,
        piko_home: &Path,
    ) -> (
        Child,
        ChildStdin,
        ThreadBridgeReceiver<TuiHostBridge, HostLine>,
    ) {
        let cwd = root.path().join("workspace");
        let helper_binary = std::env::var_os("CARGO_BIN_EXE_piko_e2e_hostd")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .unwrap_or_else(|| source_root.join("target/debug/piko-e2e-hostd"));
        let mut command = if helper_binary.is_file() {
            Command::new(helper_binary)
        } else {
            let helper_manifest = source_root.join("packages/e2e/Cargo.toml");
            let mut command = Command::new("cargo");
            command
                .args(["run", "--quiet", "--manifest-path"])
                .arg(helper_manifest)
                .args(["--bin", "piko-e2e-hostd"]);
            command
        };
        command
            .current_dir(&cwd)
            .env("PIKO_TUI_E2E_MODE", mode)
            .env("PIKO_TUI_E2E_RELEASE", release_path)
            .env("PIKO_TUI_PTY_LOG", trace_path)
            .env("PIKO_TUI_E2E_GATEWAY_LOG", gateway_log_path)
            .env("PIKO_SESSION_DIR", session_dir)
            .env("PIKO_DEV_SOURCE_ROOT", source_root)
            .env("PIKO_HOME", piko_home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = command.spawn().expect("spawn scripted hostd");
        let stdin = child.stdin.take().expect("hostd stdin");
        let stdout = child.stdout.take().expect("hostd stdout");
        let (tx, rx) = thread_bridge::<TuiHostBridge, HostLine>();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(error) => {
                        let _ = tx.send(HostLine::DecodeError(error.to_string()));
                        break;
                    }
                };
                match serde_json::from_str::<ServerMessage>(&line) {
                    Ok(message) => {
                        if tx.send(HostLine::Message(Box::new(message))).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = tx.send(HostLine::DecodeError(format!("{error}: {line}")));
                        break;
                    }
                }
            }
            let _ = tx.send(HostLine::Closed);
        });
        (child, stdin, rx)
    }

    pub fn restart(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.backlog.clear();
        let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("canonicalize source root");
        let session_dir = self.root.path().join("sessions");
        let piko_home = self.root.path().join("piko-home");
        let trace_path = self.root.path().join("trace.jsonl");
        let (child, stdin, rx) = Self::spawn_process(
            &self.mode,
            &self.root,
            &self.release_path,
            &trace_path,
            &self.gateway_log_path,
            &source_root,
            &session_dir,
            &piko_home,
        );
        self.child = child;
        self.stdin = stdin;
        self.rx = rx;
    }

    pub fn workspace(&self) -> PathBuf {
        self.root.path().join("workspace")
    }

    pub fn send(&mut self, command: HostCommand) {
        let encoded = serde_json::to_string(&command).expect("encode host command");
        writeln!(self.stdin, "{encoded}").expect("write host command");
        self.stdin.flush().expect("flush host command");
    }

    pub fn wait_for(
        &mut self,
        label: &str,
        predicate: impl Fn(&ServerMessage) -> bool,
    ) -> ServerMessage {
        if let Some(index) = self.backlog.iter().position(&predicate) {
            return self.backlog.remove(index);
        }

        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| panic!("timed out waiting for {label}"));
            match self.rx.try_recv() {
                Ok(HostLine::Message(message)) => {
                    self.observe(message.as_ref());
                    if predicate(message.as_ref()) {
                        return *message;
                    }
                    self.backlog.push(*message);
                }
                Ok(HostLine::DecodeError(error)) => panic!("hostd emitted invalid JSON: {error}"),
                Ok(HostLine::Closed) => panic!("hostd closed while waiting for {label}"),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    if remaining > Duration::ZERO {
                        thread::sleep(Duration::from_millis(10).min(remaining));
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("hostd reader closed while waiting for {label}")
                }
            }
        }
    }

    pub fn command_result(&mut self, command_id: &str) -> CommandResult {
        let message = self.wait_for("command response", |message| {
            matches!(
                message,
                ServerMessage::CommandResponse { command_id: id, .. } if id == command_id
            )
        });
        match message {
            ServerMessage::CommandResponse {
                result: Ok(result), ..
            } => result,
            ServerMessage::CommandResponse {
                result: Err(error), ..
            } => {
                panic!("command {command_id} failed: {error}")
            }
            _ => unreachable!(),
        }
    }

    pub fn command_error(&mut self, command_id: &str) -> String {
        let message = self.wait_for("command response", |message| {
            matches!(
                message,
                ServerMessage::CommandResponse { command_id: id, .. } if id == command_id
            )
        });
        match message {
            ServerMessage::CommandResponse {
                result: Err(error), ..
            } => error,
            ServerMessage::CommandResponse {
                result: Ok(result), ..
            } => panic!("command {command_id} unexpectedly succeeded: {result:?}"),
            _ => unreachable!(),
        }
    }

    pub fn create_session(&mut self, command_id: &str) -> String {
        self.send(HostCommand::SessionCreate {
            command_id: command_id.into(),
            cwd: self.workspace().display().to_string(),
        });
        let session_id = match self.command_result(command_id) {
            CommandResult::SessionCreated { session_id, .. } => session_id,
            other => panic!("expected session creation, got {other:?}"),
        };
        self.wait_for("initial session reconciliation", |message| {
            matches!(
                message,
                ServerMessage::SessionReconciled(event) if event.session_id == session_id
            )
        });
        session_id
    }

    pub fn snapshot(&mut self, session_id: &str, command_id: &str) -> SessionSnapshot {
        self.send(HostCommand::StateSnapshot {
            command_id: command_id.into(),
            session_id: session_id.into(),
        });
        self.command_result(command_id);
        match self.wait_for("session reconciliation", |message| {
            matches!(
                message,
                ServerMessage::SessionReconciled(event) if event.session_id == session_id
            )
        }) {
            ServerMessage::SessionReconciled(event) => event.snapshot,
            _ => unreachable!(),
        }
    }

    pub fn wait_started(&mut self, session_id: &str) -> String {
        let expected = self
            .latest_submitted
            .get(session_id)
            .cloned()
            .expect("submitted input receipt before work start");
        match self.wait_for("agent work start", |message| {
            matches!(
                message,
                ServerMessage::SessionReconciled(event)
                    if event.session_id == session_id
                        && event.snapshot.agent_work.iter().any(|work| {
                            work.active_work.as_ref().is_some_and(|active| {
                                active.root_input_id == expected
                            })
                        })
            )
        }) {
            ServerMessage::SessionReconciled(event) => event
                .snapshot
                .agent_work
                .into_iter()
                .find_map(|work| work.active_work.map(|active| active.root_input_id))
                .expect("active work"),
            _ => unreachable!(),
        }
    }

    pub fn wait_completed(&mut self, session_id: &str) {
        let expected = self
            .active_work
            .iter()
            .find_map(|((session, _), input_id)| (session == session_id).then(|| input_id.clone()))
            .or_else(|| self.latest_submitted.get(session_id).cloned())
            .expect("submitted input receipt before work completion");
        if !self
            .active_work
            .values()
            .any(|input_id| input_id == &expected)
            && !self.completed_work.contains(&expected)
        {
            self.wait_started(session_id);
        }
        while !self.terminal_reconciled.contains(&expected) {
            self.wait_for("agent work completion", |message| match message {
                ServerMessage::SessionReconciled(event) => event.session_id == session_id,
                ServerMessage::Usage(piko_protocol::UsageEvent::Updated {
                    session_id: id,
                    turn_id: Some(_),
                    ..
                }) => id == session_id,
                _ => false,
            });
        }
    }

    fn observe(&mut self, message: &ServerMessage) {
        if let ServerMessage::CommandResponse {
            result: Ok(CommandResult::AgentInputSubmitted { receipt, .. }),
            ..
        } = message
        {
            self.latest_submitted
                .insert(receipt.session_id.clone(), receipt.input_id.clone());
            self.input_sessions
                .insert(receipt.input_id.clone(), receipt.session_id.clone());
        }
        if let ServerMessage::Usage(piko_protocol::UsageEvent::Updated {
            turn_id: Some(root_input_id),
            ..
        }) = message
        {
            self.terminal_usage.insert(root_input_id.clone());
        }
        let ServerMessage::SessionReconciled(event) = message else {
            return;
        };
        let next = event
            .snapshot
            .agent_work
            .iter()
            .filter_map(|work| {
                work.active_work.as_ref().map(|active| {
                    (
                        (event.session_id.clone(), work.agent_instance_id.clone()),
                        active.root_input_id.clone(),
                    )
                })
            })
            .collect::<HashMap<_, _>>();
        let previous_keys = self
            .active_work
            .keys()
            .filter(|(session, _)| session == &event.session_id)
            .cloned()
            .collect::<Vec<_>>();
        for key in previous_keys {
            let previous = self.active_work.remove(&key).expect("active work key");
            if next.get(&key) != Some(&previous) {
                self.completed_work.insert(previous);
            }
        }
        for input_id in &self.terminal_usage {
            if self.input_sessions.get(input_id) == Some(&event.session_id)
                && !next.values().any(|active| active == input_id)
            {
                self.terminal_reconciled.insert(input_id.clone());
            }
        }
        self.active_work.extend(next);
    }

    pub fn release(&self) {
        std::fs::write(&self.release_path, b"release").expect("release scripted gateway");
    }

    pub fn trace(&self) -> Vec<Value> {
        let path = self.root.path().join("trace.jsonl");
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    pub fn wait_for_gateway(&self, text: &str, step: u64) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let trace = self.gateway_trace();
            if has_gateway_request(&trace, text, step) {
                return;
            }
            if Instant::now() >= deadline {
                panic!("timed out waiting for gateway request {text:?} at step {step}: {trace:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn wait_for_gateway_step(&self, step: u64) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let trace = self.gateway_trace();
            if trace.iter().any(|record| {
                record["kind"].as_str() == Some("gateway")
                    && record["value"]["step"].as_u64().unwrap_or_default() >= step
            }) {
                return;
            }
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for gateway step {step}: gateway={trace:?}; host={:?}",
                    self.trace()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn gateway_trace(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.gateway_log_path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

impl Drop for HostdHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn root_agent_id(session_id: &str) -> String {
    format!("agent_{session_id}_root")
}

pub fn has_gateway_request(trace: &[Value], text: &str, step: u64) -> bool {
    trace.iter().any(|record| {
        record["kind"].as_str() == Some("gateway")
            && record["value"]["step"].as_u64().unwrap_or_default() >= step
            && record["value"]["user_messages"].to_string().contains(text)
    })
}
