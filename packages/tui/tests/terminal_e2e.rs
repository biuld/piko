use std::{
    fs,
    io::Write,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use piko_comms::{ThreadBridgeReceiver, contracts::TuiHostBridge};
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};
use serde_json::Value;

#[path = "terminal_e2e/screen.rs"]
mod screen;
#[path = "terminal_e2e/support.rs"]
mod support;

use screen::{screen_contains, visible_screen_text};
use support::{
    binary_path, contains, read_records, server_message, spawn_reader, temp_path, trace_summary,
    unique_suffix,
};

const ROWS: u16 = 24;
const COLS: u16 = 80;
const EXIT_AFTER_MS: &str = "60000";
// Bound the user-visible guidance update after a queue or steer keypress.
const VISUAL_FEEDBACK_DEADLINE: Duration = Duration::from_secs(1);
static E2E_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct E2eHarness {
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output_rx: ThreadBridgeReceiver<TuiHostBridge, Vec<u8>>,
    output: Vec<u8>,
    log_path: PathBuf,
    gateway_log_path: PathBuf,
    release_path: PathBuf,
    session_root: PathBuf,
    piko_home: PathBuf,
}

impl E2eHarness {
    fn launch(mode: &str) -> Self {
        let suffix = unique_suffix();
        let log_path = temp_path("piko-tui-e2e-trace", suffix);
        let gateway_log_path = temp_path("piko-tui-e2e-gateway", suffix);
        let release_path = temp_path("piko-tui-e2e-release", suffix);
        let session_root = temp_path("piko-tui-e2e-session", suffix);
        let piko_home = temp_path("piko-tui-e2e-home", suffix);
        let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("canonicalize piko source root");
        let manifest = source_root.join("packages/e2e/Cargo.toml");
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open pty");

        let helper_binary = binary_path("piko-e2e-hostd");
        let mut command = CommandBuilder::new(binary_path("piko-tui"));
        command.arg("--hostd");
        if helper_binary.is_file() {
            command.arg(&helper_binary);
        } else {
            command.arg("cargo");
            for arg in ["run", "--quiet", "--manifest-path"] {
                command.arg("--hostd-arg");
                command.arg(arg);
            }
            command.arg("--hostd-arg");
            command.arg(&manifest);
            command.arg("--hostd-arg");
            command.arg("--bin");
            command.arg("--hostd-arg");
            command.arg("piko-e2e-hostd");
        }
        command.env("PIKO_TUI_EXIT_AFTER_MS", EXIT_AFTER_MS);
        command.env("PIKO_TUI_PTY_LOG", &log_path);
        command.env("PIKO_TUI_E2E_GATEWAY_LOG", &gateway_log_path);
        command.env("PIKO_TUI_E2E_MODE", mode);
        command.env("PIKO_TUI_E2E_RELEASE", &release_path);
        command.env("PIKO_SESSION_DIR", &session_root);
        command.env("PIKO_HOME", &piko_home);
        command.env("PIKO_DEV_SOURCE_ROOT", source_root);
        command.env("TERM", "xterm-256color");
        command.env_remove("COLORTERM");
        command.cwd(env!("CARGO_MANIFEST_DIR"));

        let child = pair.slave.spawn_command(command).expect("spawn TUI");
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().expect("clone pty reader");
        let output_rx = spawn_reader(reader);
        let writer = pair.master.take_writer().expect("take pty writer");
        Self {
            writer,
            child,
            output_rx,
            output: Vec::new(),
            log_path,
            gateway_log_path,
            release_path,
            session_root,
            piko_home,
        }
    }

    fn answer_keyboard_query(&mut self) {
        self.wait_for_output(b"\x1b[?u\x1b[c", Duration::from_secs(5));
        self.send(b"\x1b[?1u\x1b[?1;2c");
        self.wait_for_output(b"\x1b[?1049h", Duration::from_secs(7));
        thread::sleep(Duration::from_millis(100));
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write pty input");
        self.writer.flush().expect("flush pty input");
    }

    fn send_after(&mut self, bytes: &[u8]) {
        self.send(bytes);
        thread::sleep(Duration::from_millis(60));
    }

    fn wait_for_output(&mut self, needle: &[u8], timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            self.drain_output();
            if contains(&self.output, needle) {
                return;
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                panic!(
                    "did not observe {:?} in pty output:\n{}",
                    String::from_utf8_lossy(needle),
                    String::from_utf8_lossy(&self.output)
                );
            };
            match self.output_rx.try_recv() {
                Ok(chunk) => self.output.extend(chunk),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    thread::sleep(remaining.min(Duration::from_millis(10)));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("pty reader closed before observing output")
                }
            }
        }
    }

    fn wait_for_screen_text(&mut self, label: &str, needle: &str, timeout: Duration) -> Duration {
        let started = Instant::now();
        self.wait_for_screen_text_since(label, needle, started, timeout)
    }

    fn wait_for_screen_text_since(
        &mut self,
        label: &str,
        needle: &str,
        started: Instant,
        timeout: Duration,
    ) -> Duration {
        let deadline = started
            .checked_add(timeout)
            .expect("screen feedback deadline is representable");
        loop {
            self.drain_output();
            if screen_contains(&self.output, needle) {
                return started.elapsed();
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                panic!(
                    "did not observe {label} ({needle:?}) within {timeout:?}; visible screen:\n{}",
                    visible_screen_text(&self.output)
                );
            };
            match self.output_rx.try_recv() {
                Ok(chunk) => self.output.extend(chunk),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    thread::sleep(remaining.min(Duration::from_millis(10)));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("pty reader closed before observing {label}")
                }
            }
        }
    }

    fn drain_output(&mut self) {
        while let Ok(chunk) = self.output_rx.try_recv() {
            self.output.extend(chunk);
        }
    }

    fn wait_for_trace(&mut self, label: &str, predicate: impl Fn(&[Value]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.drain_output();
            let records = read_records(&self.log_path);
            if predicate(&records) {
                return;
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                panic!(
                    "did not observe {label} in e2e trace:\n{}\npty output:\n{}",
                    trace_summary(&records),
                    visible_screen_text(&self.output)
                );
            };
            thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }

    fn wait_for_command(&mut self, kind: &str, field: &str, text: &str) -> String {
        self.wait_for_trace("expected TUI command", |records| {
            records.iter().any(|record| {
                record["kind"].as_str() == Some("command")
                    && record["value"]["type"].as_str() == Some(kind)
                    && record["value"][field].as_str() == Some(text)
            })
        });
        read_records(&self.log_path)
            .iter()
            .find_map(|record| {
                (record["kind"].as_str() == Some("command")
                    && record["value"]["type"].as_str() == Some(kind)
                    && record["value"][field].as_str() == Some(text))
                .then(|| record["value"]["command_id"].as_str().map(str::to_string))
                .flatten()
            })
            .expect("matching e2e command has command_id")
    }

    fn wait_for_command_success(&mut self, command_id: &str) {
        self.wait_for_trace("successful command response", |records| {
            records.iter().any(|record| {
                matches!(
                    server_message(record),
                    Some(piko_protocol::ServerMessage::CommandResponse {
                        command_id: id,
                        result: Ok(piko_protocol::CommandResult::Empty),
                    }) if id == command_id
                )
            })
        });
    }

    fn wait_for_started(&mut self) {
        self.wait_for_trace("turn started", |records| {
            records.iter().any(|record| {
                matches!(
                    server_message(record),
                    Some(piko_protocol::ServerMessage::TurnLifecycle(
                        piko_protocol::TurnEvent::Started { .. }
                    ))
                )
            })
        });
    }

    fn has_completed_turn(&mut self) -> bool {
        self.drain_output();
        read_records(&self.log_path).iter().any(|record| {
            matches!(
                server_message(record),
                Some(piko_protocol::ServerMessage::TurnLifecycle(
                    piko_protocol::TurnEvent::Completed { .. }
                ))
            )
        })
    }

    fn dismiss_startup_notice(&mut self) {
        self.wait_for_screen_text(
            "startup notification",
            "session created",
            Duration::from_secs(5),
        );
        self.send_after(b"\x1b[19~");
    }

    fn wait_for_queued(&mut self) {
        self.wait_for_trace("queued turn", |records| {
            records.iter().any(|record| {
                matches!(
                    server_message(record),
                    Some(piko_protocol::ServerMessage::TurnLifecycle(
                        piko_protocol::TurnEvent::Queued { .. }
                    ))
                )
            })
        });
    }

    fn wait_for_steer_queue(&mut self) {
        self.wait_for_trace("steer queue update", |records| {
            records.iter().any(|record| {
                matches!(
                    server_message(record),
                    Some(piko_protocol::ServerMessage::Queue(
                        piko_protocol::QueueEvent::Updated { steer_count: 1, .. }
                    ))
                )
            })
        });
    }

    fn wait_for_gateway(&mut self, text: &str, step: u64) {
        let label = format!("scripted gateway request {text:?} at step {step}");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.drain_output();
            let records = read_records(&self.gateway_log_path);
            if records.iter().any(|record| {
                record["kind"].as_str() == Some("gateway")
                    && record["value"]["step"].as_u64().unwrap_or_default() >= step
                    && record["value"]["user_messages"].to_string().contains(text)
            }) {
                return;
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                panic!(
                    "did not observe {label} in gateway trace:\n{}",
                    trace_summary(&records)
                );
            };
            thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }

    fn wait_for_completed(&mut self, count: usize) {
        self.wait_for_trace("completed turns", |records| {
            records
                .iter()
                .filter(|record| {
                    matches!(
                        server_message(record),
                        Some(piko_protocol::ServerMessage::TurnLifecycle(
                            piko_protocol::TurnEvent::Completed { .. }
                        ))
                    )
                })
                .count()
                >= count
        });
    }

    fn release(&self) {
        fs::write(&self.release_path, b"release scripted gateway")
            .expect("release scripted gateway");
    }

    fn finish(&mut self) {
        self.send(b"\x04");
        let status = self.child.wait().expect("wait for TUI");
        assert!(status.success(), "TUI exited unsuccessfully: {status}");
        for _ in 0..5 {
            self.drain_output();
            thread::sleep(Duration::from_millis(25));
        }
        assert!(contains(&self.output, b"\x1b[?1049l"));
        assert!(contains(&self.output, b"\x1b[?25h"));
    }
}

impl Drop for E2eHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.log_path);
        let _ = fs::remove_file(&self.gateway_log_path);
        let _ = fs::remove_file(&self.release_path);
        let _ = fs::remove_dir_all(&self.session_root);
        let _ = fs::remove_dir_all(&self.piko_home);
    }
}

#[test]
fn steer_round_trips_from_tui_through_hostd_to_orchd() {
    let _serial = E2E_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = E2eHarness::launch("steer");
    harness.answer_keyboard_query();
    harness.send_after(b"initial work");
    harness.send_after(b"\r");
    let command_id = harness.wait_for_command("chat_submit", "text", "initial work");
    harness.wait_for_command_success(&command_id);
    harness.wait_for_gateway("initial work", 1);
    harness.wait_for_started();
    harness.wait_for_screen_text(
        "initial streaming response",
        "initial response",
        VISUAL_FEEDBACK_DEADLINE,
    );
    assert!(
        !harness.has_completed_turn(),
        "the initial visual response must arrive while the turn is still streaming"
    );
    harness.dismiss_startup_notice();

    harness.send_after(b"change course");
    let feedback_started = Instant::now();
    harness.send(b"\x1b[13;5u");
    let command_id = harness.wait_for_command("queue_steer", "message", "change course");
    harness.release();
    harness.wait_for_command_success(&command_id);
    harness.wait_for_steer_queue();
    let feedback_latency = harness.wait_for_screen_text_since(
        "steer visual feedback",
        "1 steer",
        feedback_started,
        VISUAL_FEEDBACK_DEADLINE,
    );
    assert!(
        feedback_latency <= VISUAL_FEEDBACK_DEADLINE,
        "steer feedback took {feedback_latency:?}, over {VISUAL_FEEDBACK_DEADLINE:?}"
    );
    harness.wait_for_gateway("change course", 2);
    harness.wait_for_gateway("change course", 3);
    harness.wait_for_completed(1);
    harness.finish();
}

#[test]
fn queue_round_trips_from_tui_through_hostd_to_orchd() {
    let _serial = E2E_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = E2eHarness::launch("queue");
    harness.answer_keyboard_query();
    harness.send_after(b"initial work");
    harness.send_after(b"\r");
    let command_id = harness.wait_for_command("chat_submit", "text", "initial work");
    harness.wait_for_command_success(&command_id);
    harness.wait_for_gateway("initial work", 1);
    harness.wait_for_started();
    harness.wait_for_screen_text(
        "initial streaming response",
        "initial response",
        VISUAL_FEEDBACK_DEADLINE,
    );
    assert!(
        !harness.has_completed_turn(),
        "the initial visual response must arrive while the turn is still streaming"
    );
    harness.dismiss_startup_notice();

    harness.send_after(b"queued follow-up");
    let feedback_started = Instant::now();
    harness.send(b"\x1b[13;3u");
    let command_id = harness.wait_for_command("chat_submit", "text", "queued follow-up");
    harness.wait_for_command_success(&command_id);
    harness.wait_for_queued();
    let feedback_latency = harness.wait_for_screen_text_since(
        "queue visual feedback",
        "1 queued",
        feedback_started,
        VISUAL_FEEDBACK_DEADLINE,
    );
    assert!(
        feedback_latency <= VISUAL_FEEDBACK_DEADLINE,
        "queue feedback took {feedback_latency:?}, over {VISUAL_FEEDBACK_DEADLINE:?}"
    );
    harness.release();
    harness.wait_for_gateway("queued follow-up", 2);
    harness.wait_for_completed(2);
    harness.finish();
}
