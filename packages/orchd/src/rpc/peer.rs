use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{error, info, warn};

use crate::orchestrator::core::OrchCore;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RpcId {
    String(String),
    Number(u64),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RpcInbound {
    pub jsonrpc: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub id: Option<RpcId>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum RpcOutbound {
    Request {
        jsonrpc: &'static str,
        id: RpcId,
        method: String,
        params: Value,
    },
    Notification {
        jsonrpc: &'static str,
        method: String,
        params: Value,
    },
    Success {
        jsonrpc: &'static str,
        id: RpcId,
        result: Value,
    },
    Failure {
        jsonrpc: &'static str,
        id: Option<RpcId>,
        error: RpcError,
    },
}

#[derive(Clone)]
pub struct RpcPeer {
    tx: mpsc::UnboundedSender<RpcOutbound>,
    pending: Arc<Mutex<HashMap<RpcId, oneshot::Sender<Result<Value, RpcError>>>>>,
    next_id: Arc<AtomicU64>,
}

impl RpcPeer {
    fn new(tx: mpsc::UnboundedSender<RpcOutbound>) -> Self {
        Self {
            tx,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = RpcId::Number(self.next_id.fetch_add(1, Ordering::Relaxed));
        let (done_tx, done_rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), done_tx);

        if self
            .tx
            .send(RpcOutbound::Request {
                jsonrpc: "2.0",
                id: id.clone(),
                method: method.to_string(),
                params,
            })
            .is_err()
        {
            self.pending.lock().await.remove(&id);
            return Err(RpcError {
                code: -32000,
                message: "RPC transport closed".into(),
                data: None,
            });
        }

        match done_rx.await {
            Ok(result) => result,
            Err(_) => Err(RpcError {
                code: -32000,
                message: "RPC response channel closed".into(),
                data: None,
            }),
        }
    }

    pub fn notify(&self, method: &str, params: Value) {
        let _ = self.tx.send(RpcOutbound::Notification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        });
    }

    async fn complete_pending(&self, msg: RpcInbound) -> bool {
        let Some(id) = msg.id else {
            return false;
        };
        let Some(tx) = self.pending.lock().await.remove(&id) else {
            return false;
        };
        let _ = if let Some(error) = msg.error {
            tx.send(Err(error))
        } else {
            tx.send(Ok(msg.result.unwrap_or(Value::Null)))
        };
        true
    }

    fn respond(&self, id: Option<RpcId>, result: Result<Value, RpcError>) {
        let outbound = match result {
            Ok(result) => match id {
                Some(id) => RpcOutbound::Success {
                    jsonrpc: "2.0",
                    id,
                    result,
                },
                None => return,
            },
            Err(error) => RpcOutbound::Failure {
                jsonrpc: "2.0",
                id,
                error,
            },
        };
        let _ = self.tx.send(outbound);
    }

    async fn fail_all_pending(&self, message: &str) {
        let mut pending = self.pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err(RpcError {
                code: -32000,
                message: message.to_string(),
                data: None,
            }));
        }
    }
}

pub async fn run_stdio_peer(orch: Arc<OrchCore>) {
    info!("orchd bidirectional JSON-RPC peer starting on stdio");

    let (tx, mut rx) = mpsc::unbounded_channel::<RpcOutbound>();
    let peer = Arc::new(RpcPeer::new(tx));

    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(outbound) = rx.recv().await {
            match serde_json::to_vec(&outbound) {
                Ok(bytes) => {
                    if stdout.write_all(&bytes).await.is_err()
                        || stdout.write_all(b"\n").await.is_err()
                        || stdout.flush().await.is_err()
                    {
                        break;
                    }
                }
                Err(e) => error!("failed to serialize RPC message: {e}"),
            }
        }
    });

    let peer_for_events = Arc::clone(&peer);
    let _event_cleanup = orch
        .subscribe(Box::new(move |event| {
            peer_for_events.notify("host_event", serde_json::json!({ "event": event }));
        }))
        .await;

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) if line.trim().is_empty() => continue,
            Ok(Some(line)) => {
                let msg: RpcInbound = match serde_json::from_str(&line) {
                    Ok(msg) => msg,
                    Err(e) => {
                        peer.respond(
                            None,
                            Err(RpcError {
                                code: -32700,
                                message: format!("Parse error: {e}"),
                                data: None,
                            }),
                        );
                        continue;
                    }
                };

                if msg.jsonrpc != "2.0" {
                    peer.respond(
                        msg.id,
                        Err(RpcError {
                            code: -32600,
                            message: "Invalid Request: jsonrpc must be \"2.0\"".into(),
                            data: None,
                        }),
                    );
                    continue;
                }

                if msg.method.is_none() && peer.complete_pending(msg.clone()).await {
                    continue;
                }

                let Some(method) = msg.method.clone() else {
                    warn!("dropping RPC response with no matching pending request");
                    continue;
                };
                let id = msg.id.clone();
                let params = msg.params.clone();
                let orch = Arc::clone(&orch);
                let peer = Arc::clone(&peer);
                tokio::spawn(async move {
                    let result =
                        super::handlers::dispatch(&orch, Some(peer.clone()), &method, &params)
                            .await
                            .map_err(|message| RpcError {
                                code: if message.starts_with("Invalid params") {
                                    -32602
                                } else if message.starts_with("Method not found") {
                                    -32601
                                } else {
                                    -32603
                                },
                                message,
                                data: None,
                            });
                    peer.respond(id, result);
                });
            }
            Ok(None) => break,
            Err(e) => {
                warn!("stdin read error: {e}");
                break;
            }
        }
    }

    peer.fail_all_pending("RPC transport closed").await;
    writer.abort();
    info!("orchd bidirectional JSON-RPC peer shutting down");
}
