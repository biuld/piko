//! Best-effort durable trajectory capture (F-36).
//!
//! [`TrajectoryRecorder`] implements the orchd capture port: it enqueues
//! records into a bounded channel and returns immediately, so capture never
//! blocks, fails, or alters a turn. A per-session writer task appends each
//! record as an optional (`ignorable`) journal event; dropped records are
//! counted per run and surfaced by the query. Successfully appended records
//! fan out to SSE subscribers for the real-time web viewer.
//!
//! [`TrajectoryRecorderRegistry`] is the shared per-session registry used by
//! both the turn runner (capture) and the web viewer (live fan-out and
//! dropped counts).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::infra::storage::session_store::SessionStore;
use crate::ports::TrajectoryRegistryPort;
use crate::util::now_ms;
use async_trait::async_trait;
use piko_comms::contracts::{TrajectoryLive, TrajectoryRecorders, TrajectoryWrites};
use piko_comms::{BroadcastReceiver, BroadcastSender, MailboxSender};
use piko_orchd_api::TrajectoryCapturePort;
use piko_protocol::{
    TRAJECTORY_EVENT_ASSEMBLY, TRAJECTORY_EVENT_CHILD_RUN, TRAJECTORY_EVENT_MODEL_STEP,
    TRAJECTORY_EVENT_SYSTEM_NOTIFICATION, TRAJECTORY_EVENT_TERMINAL, TRAJECTORY_EVENT_TOOL_CALL,
    TrajectoryLiveEvent, TrajectoryLiveRecordEvent, TrajectoryRecord,
};

/// Shared per-session trajectory recorder registry.
#[derive(Clone)]
pub struct TrajectoryRecorderRegistry {
    recorders: Arc<Mutex<HashMap<String, TrajectoryRecorder>>>,
    /// Bumped whenever a recorder is created, so `await_subscribe` waiters
    /// react to a session becoming observable without polling.
    created: piko_comms::LatestSender<TrajectoryRecorders, u64>,
    generation: Arc<AtomicU64>,
}

impl Default for TrajectoryRecorderRegistry {
    fn default() -> Self {
        let (created, _) = piko_comms::latest::<TrajectoryRecorders, u64>(0);
        Self {
            recorders: Arc::new(Mutex::new(HashMap::new())),
            created,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl TrajectoryRecorderRegistry {
    /// Process-wide shared registry. All runners and the web viewer share the
    /// same recorder map, so a runner rebuild (auth/model changes) never
    /// orphans the live SSE or dropped-count views.
    pub fn global() -> Self {
        static GLOBAL: OnceLock<TrajectoryRecorderRegistry> = OnceLock::new();
        GLOBAL
            .get_or_init(TrajectoryRecorderRegistry::default)
            .clone()
    }

    pub fn get_or_create(&self, session_id: &str, store: SessionStore) -> TrajectoryRecorder {
        let mut recorders = self.recorders.lock().unwrap();
        if !recorders.contains_key(session_id) {
            // Wake waiters before inserting: a waiter that observes the bump
            // re-checks the map and finds the recorder.
            let next = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
            self.created.send_replace(next);
        }
        recorders
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

#[async_trait]
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

    fn subscribe(
        &self,
        session_id: &str,
    ) -> Option<BroadcastReceiver<TrajectoryLive, TrajectoryLiveEvent>> {
        self.recorders
            .lock()
            .unwrap()
            .get(session_id)
            .map(|recorder| recorder.subscribe())
    }

    async fn await_subscribe(
        &self,
        session_id: &str,
    ) -> Option<BroadcastReceiver<TrajectoryLive, TrajectoryLiveEvent>> {
        let mut created = self.created.subscribe();
        loop {
            created.borrow_and_update();
            if let Some(recorder) = self.get(session_id) {
                return Some(recorder.subscribe());
            }
            if created.changed().await.is_err() {
                return None;
            }
        }
    }
}

struct TrajectoryWrite {
    store: SessionStore,
    session_id: String,
    commit_id: String,
    committed_at: i64,
    event_type: &'static str,
    payload: serde_json::Value,
    root_input_id: String,
    record: TrajectoryRecord,
}

/// Per-session trajectory capture port + live fan-out handle.
#[derive(Clone)]
pub struct TrajectoryRecorder {
    store: SessionStore,
    session_id: String,
    tx: MailboxSender<TrajectoryWrites, TrajectoryWrite>,
    dropped: Arc<Mutex<HashMap<String, u32>>>,
    live: BroadcastSender<TrajectoryLive, TrajectoryLiveEvent>,
    sequence: Arc<AtomicU64>,
}

impl TrajectoryRecorder {
    pub fn new(store: SessionStore, session_id: String) -> Self {
        let (tx, mut rx) = piko_comms::mailbox::<TrajectoryWrites, TrajectoryWrite>();
        let (live, _) = piko_comms::broadcast::<TrajectoryLive, TrajectoryLiveEvent>();
        let dropped = Arc::new(Mutex::new(HashMap::new()));
        let dropped_writer = Arc::clone(&dropped);
        let live_writer = live.clone();
        tokio::spawn(async move {
            while let Some(write) = rx.recv().await {
                // A run's first record (assembly) or its terminal record means
                // the session's run list changed: publish a marker so viewers
                // following a different run refresh the strip too.
                let runs_changed = matches!(
                    write.event_type,
                    TRAJECTORY_EVENT_ASSEMBLY | TRAJECTORY_EVENT_TERMINAL
                );
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
                match result {
                    Ok(Ok(revision)) => {
                        let session_id = write.session_id.clone();
                        let root_input_id = write.root_input_id;
                        let _ = live_writer.send(TrajectoryLiveEvent::Record(Box::new(
                            TrajectoryLiveRecordEvent {
                                session_id: session_id.clone(),
                                root_input_id,
                                revision,
                                committed_at,
                                record: write.record,
                            },
                        )));
                        if runs_changed {
                            let _ = live_writer.send(TrajectoryLiveEvent::RunsChanged {
                                session_id,
                                committed_at,
                            });
                        }
                    }
                    _ => {
                        let mut dropped = dropped_writer.lock().unwrap();
                        *dropped.entry(write.root_input_id).or_insert(0) += 1;
                    }
                }
            }
        });
        Self {
            store,
            session_id,
            tx,
            dropped,
            live,
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

    pub fn subscribe(&self) -> BroadcastReceiver<TrajectoryLive, TrajectoryLiveEvent> {
        self.live.subscribe()
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
            session_id: self.session_id.clone(),
            commit_id,
            committed_at: now_ms(),
            event_type,
            payload,
            root_input_id,
            record,
        };
        let root_input_id_for_drop = write.root_input_id.clone();
        if self.tx.try_send(write).is_err() {
            let mut dropped = self.dropped.lock().unwrap();
            *dropped.entry(root_input_id_for_drop).or_insert(0) += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn await_subscribe_resolves_when_recorder_appears() {
        let registry = TrajectoryRecorderRegistry::default();
        let wait = registry.await_subscribe("s1");
        tokio::pin!(wait);
        // Must not resolve while no recorder exists for the session.
        tokio::select! {
            _result = &mut wait => panic!("resolved without a recorder"),
            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
        }
        let temp = tempfile::tempdir().unwrap();
        let store =
            SessionStore::create_session(temp.path(), "s1".into(), "/project".into(), 1).unwrap();
        registry.get_or_create("s1", store);
        let recorder = registry.get("s1").expect("recorder exists");
        let mut receiver = (&mut wait).await.expect("resolves once a recorder appears");
        piko_orchd_api::TrajectoryCapturePort::record(
            &recorder,
            TrajectoryRecord::Terminal(piko_protocol::TrajectoryTerminalRecord {
                identity: piko_protocol::TrajectoryIdentity {
                    session_id: "s1".into(),
                    agent_instance_id: "a".into(),
                    root_input_id: "input-r1".into(),
                },
                kind: piko_protocol::TrajectoryTerminalKind::Completed,
                reason: None,
                finished_at: 1,
            }),
        )
        .await;
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
            .await
            .expect("record fans out to the subscriber")
            .expect("broadcast channel open");
        assert!(matches!(event, TrajectoryLiveEvent::Record(_)));
    }
}
