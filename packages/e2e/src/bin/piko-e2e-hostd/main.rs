//! Standalone hostd fixture used by cross-process E2E tests.

mod scripted;

use std::{
    io::{self},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use piko_hostd::{
    domain::config::HostSettings,
    protocol::{HostServer, run_jsonl_server},
};
use piko_llmd::gateway::InferenceGateway;
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader, ReadBuf};

use scripted::{ScriptMode, ScriptedGateway, append_record, build_runner};

struct LoggingReader<R: AsyncRead> {
    inner: BufReader<R>,
    log_path: PathBuf,
    pending: Vec<u8>,
}

impl<R: AsyncRead> LoggingReader<R> {
    fn new(inner: R, log_path: PathBuf) -> Self {
        Self {
            inner: BufReader::new(inner),
            log_path,
            pending: Vec::new(),
        }
    }

    fn record_complete_lines(&mut self) {
        while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.pending.drain(..=index).collect();
            if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                append_record(&self.log_path, "command", value);
            }
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for LoggingReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buffer)
    }
}

impl<R: AsyncRead + Unpin> AsyncBufRead for LoggingReader<R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        Pin::new(&mut self.get_mut().inner).poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amount: usize) {
        let bytes = self.inner.buffer()[..amount].to_vec();
        self.inner.consume(amount);
        self.pending.extend(bytes);
        self.record_complete_lines();
    }
}

struct LoggingWriter<W> {
    inner: W,
    log_path: PathBuf,
    pending: Vec<u8>,
}

impl<W> LoggingWriter<W> {
    fn new(inner: W, log_path: PathBuf) -> Self {
        Self {
            inner,
            log_path,
            pending: Vec::new(),
        }
    }

    fn record_complete_lines(&mut self) {
        while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.pending.drain(..=index).collect();
            if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                append_record(&self.log_path, "event", value);
            }
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for LoggingWriter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::new(&mut self.inner).poll_write(cx, buffer) {
            Poll::Ready(Ok(written)) => {
                self.pending.extend_from_slice(&buffer[..written]);
                self.record_complete_lines();
                Poll::Ready(Ok(written))
            }
            other => other,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = ScriptMode::parse(&std::env::var("PIKO_TUI_E2E_MODE")?)?;
    let release_path = PathBuf::from(std::env::var("PIKO_TUI_E2E_RELEASE")?);
    let log_path = PathBuf::from(std::env::var("PIKO_TUI_PTY_LOG")?);
    let gateway_log_path = std::env::var_os("PIKO_TUI_E2E_GATEWAY_LOG")
        .map(PathBuf::from)
        .unwrap_or_else(|| log_path.clone());
    let session_root = PathBuf::from(std::env::var("PIKO_SESSION_DIR")?);

    let gateway = Arc::new(ScriptedGateway::new(mode, release_path, gateway_log_path));
    let runner = build_runner(gateway.clone(), mode).await;
    let server = HostServer::with_storage_runner_settings(
        piko_hostd::infra::storage::JsonlSessionRepository::new(session_root),
        runner,
        HostSettings::default(),
    );
    if mode.uses_model_executor() {
        let gateway: Arc<dyn InferenceGateway> = gateway;
        server.set_model_executor(gateway).await;
    }
    run_jsonl_server(
        LoggingReader::new(tokio::io::stdin(), log_path.clone()),
        LoggingWriter::new(tokio::io::stdout(), log_path),
        server,
    )
    .await
}
