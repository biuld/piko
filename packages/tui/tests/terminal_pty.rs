use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use piko_comms::{ThreadBridgeReceiver, contracts::TuiHostBridge, thread_bridge};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::Value;

const INITIAL_ROWS: u16 = 24;
const INITIAL_COLS: u16 = 80;
const EXIT_AFTER_MS: &str = "10000";

struct PtyHarness {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output_rx: ThreadBridgeReceiver<TuiHostBridge, Vec<u8>>,
    output: Vec<u8>,
    log_path: PathBuf,
    piko_home: PathBuf,
}

impl PtyHarness {
    fn launch() -> Self {
        let suffix = unique_suffix();
        let log_path = std::env::temp_dir().join(format!(
            "piko-tui-pty-{}-{}.jsonl",
            std::process::id(),
            suffix
        ));
        let piko_home =
            std::env::temp_dir().join(format!("piko-tui-pty-home-{}-{suffix}", std::process::id()));
        let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("canonicalize piko source root");
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: INITIAL_ROWS,
                cols: INITIAL_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open pty");

        let mut command = CommandBuilder::new(binary_path("piko-tui"));
        command.arg("--hostd");
        command.arg(binary_path("piko-tui-pty-hostd"));
        command.env("PIKO_TUI_EXIT_AFTER_MS", EXIT_AFTER_MS);
        command.env("PIKO_TUI_PTY_LOG", &log_path);
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
            master: pair.master,
            writer,
            child,
            output_rx,
            output: Vec::new(),
            log_path,
            piko_home,
        }
    }

    fn answer_keyboard_query(&mut self, enhanced: bool) {
        self.wait_for_output(b"\x1b[?u\x1b[c", Duration::from_secs(5));
        if enhanced {
            self.send(b"\x1b[?1u\x1b[?1;2c");
        }
        self.wait_for_output(b"\x1b[?1049h", Duration::from_secs(7));
        thread::sleep(Duration::from_millis(100));
    }

    fn send(&mut self, bytes: &[u8]) {
        if let Err(error) = self
            .writer
            .write_all(bytes)
            .and_then(|()| self.writer.flush())
        {
            self.drain_output();
            let status = self.child.try_wait().ok().flatten();
            panic!(
                "write pty input: {error}; child status: {status:?}; output:\n{}; commands: {:?}",
                String::from_utf8_lossy(&self.output),
                read_commands(&self.log_path)
            );
        }
    }

    fn send_after(&mut self, bytes: &[u8], delay: Duration) {
        self.send(bytes);
        thread::sleep(delay);
    }

    fn resize(&self, rows: u16, cols: u16) {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("resize pty");
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

    fn drain_output(&mut self) {
        while let Ok(chunk) = self.output_rx.try_recv() {
            self.output.extend(chunk);
        }
    }

    fn wait_for_chat_submit(&mut self, expected_text: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.drain_output();
            if read_commands(&self.log_path).iter().any(|command| {
                command.get("type").and_then(Value::as_str) == Some("agent_input_submit")
                    && command
                        .get("input")
                        .and_then(|input| input.get("content"))
                        .and_then(Value::as_str)
                        == Some(expected_text)
            }) {
                return;
            }
            if Instant::now() >= deadline {
                panic!(
                    "mock hostd did not receive agent_input_submit with {expected_text:?}; commands: {:?}",
                    read_commands(&self.log_path)
                );
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn wait_for_output_growth(&mut self, previous_len: usize, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            self.drain_output();
            if self.output.len() > previous_len {
                return;
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                panic!("pty produced no repaint after resize");
            };
            match self.output_rx.try_recv() {
                Ok(chunk) => self.output.extend(chunk),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    thread::sleep(remaining.min(Duration::from_millis(10)));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("pty reader closed before resize repaint")
                }
            }
        }
    }

    fn finish(&mut self) {
        self.send(b"\x04");
        let status = self.child.wait().expect("wait for TUI");
        assert!(status.success(), "TUI exited unsuccessfully: {status}");
        for _ in 0..5 {
            self.drain_output();
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            contains(&self.output, b"\x1b[?1049l"),
            "normal cleanup must leave the alternate screen"
        );
        assert!(
            contains(&self.output, b"\x1b[?25h"),
            "normal cleanup must show the cursor"
        );
    }
}

impl Drop for PtyHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.log_path);
        let _ = fs::remove_dir_all(&self.piko_home);
    }
}

#[test]
fn enhanced_pty_routes_paste_shift_enter_resize_and_cleanup() {
    let mut harness = PtyHarness::launch();
    harness.answer_keyboard_query(true);

    harness.send_after(b"\x1b[200~pasted", Duration::from_millis(60));
    harness.send_after(b"\x1b[201~", Duration::from_millis(60));
    harness.send_after(b"\x1b[13;2u", Duration::from_millis(60));
    harness.send_after(b"next", Duration::from_millis(60));
    harness.send_after(b"\r", Duration::from_millis(60));
    harness.wait_for_chat_submit("pasted\nnext");

    let output_len = harness.output.len();
    harness.resize(30, 100);
    harness.wait_for_output_growth(output_len, Duration::from_secs(2));
    harness.finish();
}

#[test]
fn baseline_pty_routes_ctrl_j_as_newline_and_cleans_up() {
    let mut harness = PtyHarness::launch();
    harness.answer_keyboard_query(false);

    harness.send_after(b"pasted", Duration::from_millis(60));
    harness.send_after(b"\n", Duration::from_millis(60));
    harness.send_after(b"next", Duration::from_millis(60));
    harness.send_after(b"\r", Duration::from_millis(60));
    harness.wait_for_chat_submit("pasted\nnext");
    harness.finish();
}

fn spawn_reader(mut reader: Box<dyn Read + Send>) -> ThreadBridgeReceiver<TuiHostBridge, Vec<u8>> {
    let (sender, receiver) = thread_bridge::<TuiHostBridge, Vec<u8>>();
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if sender.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    receiver
}

fn read_commands(path: &PathBuf) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn binary_path(name: &str) -> PathBuf {
    let env_name = format!("CARGO_BIN_EXE_{name}");
    std::env::var_os(env_name)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/debug")
                .join(name)
        })
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos()
}
