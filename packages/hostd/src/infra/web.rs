//! Read-only loopback trajectory web viewer (F-36).
//!
//! Serves the static viewer page plus JSON/SSE endpoints over loopback HTTP.
//! All endpoints are read-only: they never mutate session state or invoke
//! the model gateway. Bind failure is non-fatal: hostd logs and continues.

use std::convert::Infallible;
use std::sync::Arc;

use async_stream::stream;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use futures_core::Stream;
use piko_protocol::{TrajectoryLiveEvent, TrajectoryRun, TrajectoryRunListPage};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::application::TrajectoryQuery;
use crate::domain::config::TrajectorySettings;
use crate::ports::TrajectoryRegistryPort;
use crate::ports::session_repository::SessionRepositoryPort;

const VIEWER_HTML: &str = include_str!("../../assets/index.html");
const VIEWER_CSS: &str = include_str!("../../assets/viewer.css");
const JS_FORMAT: &str = include_str!("../../assets/js/format.js");
const JS_API: &str = include_str!("../../assets/js/api.js");
const JS_STORE: &str = include_str!("../../assets/js/store.js");
const JS_PANELS: &str = include_str!("../../assets/js/panels.js");
const JS_MESSAGES: &str = include_str!("../../assets/js/messages.js");
const JS_TIMELINE: &str = include_str!("../../assets/js/timeline.js");
const JS_PROMPT: &str = include_str!("../../assets/js/prompt.js");
const JS_APP: &str = include_str!("../../assets/js/app.js");

#[derive(Clone)]
pub struct TrajectoryWebState {
    query: TrajectoryQuery,
    registry: Arc<dyn TrajectoryRegistryPort>,
    storage: Option<Arc<dyn SessionRepositoryPort>>,
}

impl TrajectoryWebState {
    pub fn new(
        query: TrajectoryQuery,
        registry: Arc<dyn TrajectoryRegistryPort>,
        storage: Option<Arc<dyn SessionRepositoryPort>>,
    ) -> Self {
        Self {
            query,
            registry,
            storage,
        }
    }
}

#[derive(Deserialize)]
struct RunListQuery {
    session_id: String,
    #[serde(default)]
    agent_instance_id: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct RunIdQuery {
    session_id: String,
}

async fn index() -> Html<&'static str> {
    Html(VIEWER_HTML)
}

async fn css_asset() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        VIEWER_CSS,
    )
}

async fn js_asset(Path(file): Path<String>) -> impl IntoResponse {
    let body = match file.as_str() {
        "format.js" => JS_FORMAT,
        "api.js" => JS_API,
        "store.js" => JS_STORE,
        "panels.js" => JS_PANELS,
        "messages.js" => JS_MESSAGES,
        "timeline.js" => JS_TIMELINE,
        "prompt.js" => JS_PROMPT,
        "app.js" => JS_APP,
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

async fn sessions(State(state): State<TrajectoryWebState>) -> Json<Vec<String>> {
    let mut seen = std::collections::HashSet::new();
    let mut ordered: Vec<(String, String)> = Vec::new();
    if let Some(storage) = &state.storage
        && let Ok(summaries) = storage.summaries(None).await
    {
        for summary in summaries {
            seen.insert(summary.session_id.clone());
            ordered.push((
                summary.session_id.clone(),
                summary.modified_at.unwrap_or_default(),
            ));
        }
    }
    for session_id in state.query.session_paths.lock().await.keys().cloned() {
        if seen.insert(session_id.clone()) {
            ordered.push((session_id, String::new()));
        }
    }
    // Most recently modified first, so the viewer defaults to the latest
    // resumed/active session. Compare numerically: timestamps may differ in
    // digit length, so lexicographic ordering would be wrong.
    ordered.sort_by(|left, right| {
        let left_ms = left.1.parse::<i64>().unwrap_or(0);
        let right_ms = right.1.parse::<i64>().unwrap_or(0);
        right_ms.cmp(&left_ms)
    });
    let sessions = ordered
        .into_iter()
        .map(|(session_id, _)| session_id)
        .collect();
    Json(sessions)
}

async fn list_runs(
    State(state): State<TrajectoryWebState>,
    Query(params): Query<RunListQuery>,
) -> Result<Json<TrajectoryRunListPage>, (StatusCode, String)> {
    let dropped = state.registry.dropped_counts(&params.session_id);
    state
        .query
        .list_runs(
            &params.session_id,
            params.agent_instance_id.as_deref(),
            params.cursor.as_deref(),
            params.limit.unwrap_or(0),
            &dropped,
        )
        .await
        .map(Json)
        .map_err(|error| (StatusCode::NOT_FOUND, error.to_string()))
}

async fn fetch_run(
    State(state): State<TrajectoryWebState>,
    Path(run_id): Path<String>,
    Query(params): Query<RunIdQuery>,
) -> Result<Json<TrajectoryRun>, (StatusCode, String)> {
    let dropped = state.registry.dropped_counts(&params.session_id);
    state
        .query
        .fetch_run(&params.session_id, &run_id, &dropped)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::NOT_FOUND, error.to_string()))
}

async fn stream_run(
    State(state): State<TrajectoryWebState>,
    Path(run_id): Path<String>,
    Query(params): Query<RunIdQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Wait for a recorder instead of subscribing once: a viewer opened before
    // this process attached the session (or right after hostd restart) would
    // otherwise never see live records — the no-recorder stream hung on
    // keep-alive pings forever.
    let receiver = state.registry.await_subscribe(&params.session_id).await;
    let stream = stream! {
        let Some(mut receiver) = receiver else {
            // No recorder for this session and none can appear; keep the
            // connection open with keep-alive pings instead of emitting a
            // reload event, which would make the client refetch in an
            // infinite reconnect loop.
            std::future::pending::<()>().await;
            return;
        };
        loop {
            match receiver.recv().await {
                Ok(ref live @ TrajectoryLiveEvent::Record(ref event)) if event.run_id == run_id => {
                    if let Ok(data) = serde_json::to_string(live) {
                        yield Ok(Event::default().id(event.revision.to_string()).data(data));
                    }
                }
                // Run-list changes (a run started/finished in this session)
                // reach every per-run stream regardless of the watched run.
                Ok(ref live @ TrajectoryLiveEvent::RunsChanged { .. }) => {
                    if let Ok(data) = serde_json::to_string(live) {
                        yield Ok(Event::default().data(data));
                    }
                }
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => {
                    yield Ok(Event::default().data("reload"));
                }
                Err(RecvError::Closed) => break,
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Bind and serve the viewer on loopback when enabled. Returns the server
/// task so the caller keeps it alive. Bind failure is non-fatal.
pub fn spawn(
    settings: &TrajectorySettings,
    state: TrajectoryWebState,
) -> Option<tokio::task::JoinHandle<std::io::Result<()>>> {
    if !settings.enabled.unwrap_or(false) {
        return None;
    }
    let bind = settings.bind.clone().unwrap_or_else(|| "127.0.0.1".into());
    let port = settings.port.unwrap_or(3847);
    let app = Router::new()
        .route("/", get(index))
        .route("/assets/viewer.css", get(css_asset))
        .route("/assets/js/{file}", get(js_asset))
        .route("/api/trajectory/sessions", get(sessions))
        .route("/api/trajectory/runs", get(list_runs))
        .route("/api/trajectory/runs/{run_id}", get(fetch_run))
        .route("/api/trajectory/runs/{run_id}/stream", get(stream_run))
        .with_state(state.clone());
    let addr = format!("{bind}:{port}");
    Some(tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("piko trajectory viewer failed to bind {addr}: {error}");
                return Err(error);
            }
        };
        eprintln!("piko trajectory viewer: http://{addr}/");
        axum::serve(listener, app).await
    }))
}
