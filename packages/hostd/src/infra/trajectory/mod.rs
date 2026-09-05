//! Best-effort durable trajectory capture (F-36).
//!
//! [`TrajectoryRecorder`] implements the orchd capture port: it enqueues
//! records into a bounded channel and returns immediately, so capture never
//! blocks, fails, or alters a turn. A per-session writer task appends each
//! record as an optional (`ignorable`) journal event; dropped records are
//! counted per run. Session History reads those records as diagnostics.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::infra::storage::session_store::SessionStore;
use crate::ports::TrajectoryRegistryPort;
use crate::util::now_ms;
use async_trait::async_trait;
use piko_comms::MailboxSender;
use piko_comms::contracts::TrajectoryWrites;
use piko_orchd_api::TrajectoryCapturePort;
use piko_protocol::{
    TRAJECTORY_EVENT_ASSEMBLY, TRAJECTORY_EVENT_CHILD_RUN, TRAJECTORY_EVENT_MODEL_STEP,
    TRAJECTORY_EVENT_SYSTEM_NOTIFICATION, TRAJECTORY_EVENT_TERMINAL, TRAJECTORY_EVENT_TOOL_CALL,
    TrajectoryRecord,
};

/// Shared per-session trajectory recorder registry.
#[derive(Clone)]
pub struct TrajectoryRecorderRegistry {
    recorders: Arc<Mutex<HashMap<String, TrajectoryRecorder>>>,
}

impl Default for TrajectoryRecorderRegistry {
    fn default() -> Self {
        Self {
            recorders: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl TrajectoryRecorderRegistry {
    /// Process-wide shared registry. All runners share the same recorder map,
    /// so a runner rebuild (auth/model changes) never orphans capture.
    pub fn global() -> Self {
        static GLOBAL: OnceLock<TrajectoryRecorderRegistry> = OnceLock::new();
        GLOBAL
            .get_or_init(TrajectoryRecorderRegistry::default)
            .clone()
    }

    pub fn get_or_create(&self, session_id: &str, store: SessionStore) -> TrajectoryRecorder {
        self.recorders
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_insert_with(|| TrajectoryRecorder::new(store, session_id.to_string()))
            .clone()
    }

    pub fn get(&self, session_id: &str) -> Option<TrajectoryRecorder> {
        self.recorders.lock().unwrap().get(session_id).cloned()
    }

    pub fn all(&self) -> HashMap<String, TrajectoryRecorder> {
        self.recorders.lock().unwrap().clone()
    }
}

impl TrajectoryRegistryPort for TrajectoryRecorderRegistry {
    fn get(&self, session_id: &str) -> Option<Arc<dyn TrajectoryCapturePort>> {
        self.recorders
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .map(|recorder| Arc::new(recorder) as Arc<dyn TrajectoryCapturePort>)
    }

    fn dropped_counts(&self, session_id: &str) -> HashMap<String, u32> {
        self.recorders
            .lock()
            .unwrap()
            .get(session_id)
            .map(|recorder| recorder.dropped_counts())
            .unwrap_or_default()
    }
}

struct TrajectoryWrite {
    store: SessionStore,
    commit_id: String,
    committed_at: i64,
    event_type: &'static str,
    payload: serde_json::Value,
    root_input_id: String,
}

/// Per-session trajectory capture port.
#[derive(Clone)]
pub struct TrajectoryRecorder {
    store: SessionStore,
    session_id: String,
    tx: MailboxSender<TrajectoryWrites, TrajectoryWrite>,
    dropped: Arc<Mutex<HashMap<String, u32>>>,
    sequence: Arc<AtomicU64>,
}

impl TrajectoryRecorder {
    pub fn new(store: SessionStore, session_id: String) -> Self {
        let (tx, mut rx) = piko_comms::mailbox::<TrajectoryWrites, TrajectoryWrite>();
        let dropped = Arc::new(Mutex::new(HashMap::new()));
        let dropped_writer = Arc::clone(&dropped);
        tokio::spawn(async move {
            while let Some(write) = rx.recv().await {
                let store = write.store.clone();
                let commit_id = write.commit_id.clone();
                let committed_at = write.committed_at;
                let event_type = write.event_type.to_string();
                let payload = write.payload.clone();
                let result = tokio::task::spawn_blocking(move || {
                    store.with_io(|| {
                        store.append_optional_event(&commit_id, committed_at, &event_type, payload)
                    })
                })
                .await;
                if !matches!(result, Ok(Ok(_))) {
                    let mut dropped = dropped_writer.lock().unwrap();
                    *dropped.entry(write.root_input_id).or_insert(0) += 1;
                }
            }
        });
        Self {
            store,
            session_id,
            tx,
            dropped,
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Per-run count of records dropped because the queue was full or an
    /// append failed. Best-effort capture never retries or blocks a turn.
    pub fn dropped_counts(&self) -> HashMap<String, u32> {
        self.dropped
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

/// Split a record into journal event type, root input identity, and the serialized
/// inner DTO (not the tagged enum wrapper).
fn split_record(record: &TrajectoryRecord) -> Option<(&'static str, String, serde_json::Value)> {
    match record {
        TrajectoryRecord::Assembly(record) => Some((
            TRAJECTORY_EVENT_ASSEMBLY,
            record.identity.root_input_id.clone(),
            serde_json::to_value(record).ok()?,
        )),
        TrajectoryRecord::ModelStep(record) => Some((
            TRAJECTORY_EVENT_MODEL_STEP,
            record.identity.root_input_id.clone(),
            serde_json::to_value(record).ok()?,
        )),
        TrajectoryRecord::ToolCall(record) => Some((
            TRAJECTORY_EVENT_TOOL_CALL,
            record.identity.root_input_id.clone(),
            serde_json::to_value(record).ok()?,
        )),
        TrajectoryRecord::ChildRun(record) => Some((
            TRAJECTORY_EVENT_CHILD_RUN,
            record.identity.root_input_id.clone(),
            serde_json::to_value(record).ok()?,
        )),
        TrajectoryRecord::SystemNotification(record) => Some((
            TRAJECTORY_EVENT_SYSTEM_NOTIFICATION,
            record.identity.root_input_id.clone(),
            serde_json::to_value(record).ok()?,
        )),
        TrajectoryRecord::Terminal(record) => Some((
            TRAJECTORY_EVENT_TERMINAL,
            record.identity.root_input_id.clone(),
            serde_json::to_value(record).ok()?,
        )),
    }
}

#[async_trait]
impl TrajectoryCapturePort for TrajectoryRecorder {
    async fn record(&self, record: TrajectoryRecord) {
        let Some((event_type, root_input_id, payload)) = split_record(&record) else {
            // Serialization of a protocol DTO cannot fail; count as dropped.
            let mut dropped = self.dropped.lock().unwrap();
            *dropped.entry("unparseable".to_string()).or_insert(0) += 1;
            return;
        };
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let commit_id = piko_orchd_api::stable_internal_id(
            "trajectory",
            &[&self.session_id, &root_input_id, &sequence.to_string()],
        );
        let write = TrajectoryWrite {
            store: self.store.clone(),
            commit_id,
            committed_at: now_ms(),
            event_type,
            payload,
            root_input_id,
        };
        let root_input_id_for_drop = write.root_input_id.clone();
        if self.tx.try_send(write).is_err() {
            let mut dropped = self.dropped.lock().unwrap();
            *dropped.entry(root_input_id_for_drop).or_insert(0) += 1;
        }
    }
}
