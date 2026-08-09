use std::pin::Pin;

use async_stream::stream;
use futures::{Stream, StreamExt};
use reqwest::{Response, StatusCode};
use tokio_util::sync::CancellationToken;

use crate::gateway::{ErrorClass, InferenceError};
use crate::target::ModelTarget;

const MAX_ERROR_BODY: usize = 16 * 1024;
const MAX_JSON_BODY: usize = 32 * 1024 * 1024;
const MAX_SSE_EVENT: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseMessage {
    pub data: String,
    pub done: bool,
}

pub async fn send(
    client: &reqwest::Client,
    target: &ModelTarget,
    body: &serde_json::Value,
    stream: bool,
    cancel: &CancellationToken,
) -> Result<Response, InferenceError> {
    let request = client
        .post(target.endpoint.clone())
        .headers(target.headers.clone())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::ACCEPT,
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .json(body);
    let response = tokio::select! {
        _ = cancel.cancelled() => return Err(InferenceError::cancelled(&target.id)),
        result = request.send() => result.map_err(|error| transport_error(target, error))?,
    };
    if response.status().is_success() {
        return Ok(response);
    }
    Err(upstream_error(target, response, cancel).await)
}

pub async fn json(
    response: Response,
    target: &ModelTarget,
    cancel: &CancellationToken,
) -> Result<serde_json::Value, InferenceError> {
    let bytes = read_bounded(response, MAX_JSON_BODY, target, cancel).await?;
    serde_json::from_slice(&bytes).map_err(|error| {
        InferenceError::new(
            ErrorClass::Protocol,
            &target.id,
            "decode_json",
            format!("malformed JSON response: {error}"),
        )
    })
}

pub fn sse(
    response: Response,
    target: String,
    cancel: CancellationToken,
) -> Pin<Box<dyn Stream<Item = Result<SseMessage, InferenceError>> + Send>> {
    Box::pin(stream! {
        let mut source = response.bytes_stream();
        let mut decoder = SseDecoder::new(target.clone());
        loop {
            let next = tokio::select! {
                _ = cancel.cancelled() => {
                    yield Err(InferenceError::cancelled(&target));
                    return;
                }
                next = source.next() => next,
            };
            match next {
                Some(Ok(bytes)) => match decoder.push(&bytes) {
                    Ok(messages) => {
                        for message in messages {
                            let done = message.done;
                            yield Ok(message);
                            if done { return; }
                        }
                    }
                    Err(error) => { yield Err(error); return; }
                },
                Some(Err(error)) => { yield Err(transport_error_for(&target, error)); return; }
                None => match decoder.finish() {
                    Ok(messages) => {
                        for message in messages { yield Ok(message); }
                        return;
                    }
                    Err(error) => { yield Err(error); return; }
                },
            }
        }
    })
}

struct SseDecoder {
    target: String,
    bytes: Vec<u8>,
    data: Vec<String>,
}

impl SseDecoder {
    fn new(target: String) -> Self {
        Self {
            target,
            bytes: Vec::new(),
            data: Vec::new(),
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseMessage>, InferenceError> {
        self.bytes.extend_from_slice(bytes);
        if self.bytes.len() > MAX_SSE_EVENT {
            return Err(self.error("SSE event exceeds size limit"));
        }
        let mut messages = Vec::new();
        while let Some(position) = self.bytes.iter().position(|byte| *byte == b'\n') {
            let mut line = self.bytes.drain(..=position).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line =
                String::from_utf8(line).map_err(|_| self.error("SSE contains invalid UTF-8"))?;
            if let Some(message) = self.line(&line)? {
                messages.push(message);
            }
        }
        Ok(messages)
    }

    fn line(&mut self, line: &str) -> Result<Option<SseMessage>, InferenceError> {
        if line.is_empty() {
            if self.data.is_empty() {
                return Ok(None);
            }
            let data = self.data.join("\n");
            self.data.clear();
            return Ok(Some(SseMessage {
                done: data.trim() == "[DONE]",
                data,
            }));
        }
        if line.starts_with(':') {
            return Ok(None);
        }
        let (field, mut value) = line.split_once(':').unwrap_or((line, ""));
        if value.starts_with(' ') {
            value = &value[1..];
        }
        if field == "data" {
            self.data.push(value.to_string());
            if self.data.iter().map(String::len).sum::<usize>() > MAX_SSE_EVENT {
                return Err(self.error("SSE event exceeds size limit"));
            }
        }
        Ok(None)
    }

    fn finish(&mut self) -> Result<Vec<SseMessage>, InferenceError> {
        let mut messages = Vec::new();
        if !self.bytes.is_empty() {
            let line = String::from_utf8(std::mem::take(&mut self.bytes))
                .map_err(|_| self.error("SSE contains invalid UTF-8"))?;
            if let Some(message) = self.line(line.trim_end_matches('\r'))? {
                messages.push(message);
            }
        }
        if !self.data.is_empty() {
            let data = self.data.join("\n");
            self.data.clear();
            messages.push(SseMessage {
                done: data.trim() == "[DONE]",
                data,
            });
        }
        Ok(messages)
    }

    fn error(&self, message: impl Into<String>) -> InferenceError {
        InferenceError::new(ErrorClass::Sse, &self.target, "decode_sse", message)
    }
}

fn transport_error(target: &ModelTarget, error: reqwest::Error) -> InferenceError {
    transport_error_for(&target.id, error)
}

fn transport_error_for(target: &str, error: reqwest::Error) -> InferenceError {
    let class = if error.is_timeout() {
        ErrorClass::Timeout
    } else {
        ErrorClass::Transport
    };
    InferenceError::new(class, target, "http", error.to_string())
}

async fn read_bounded(
    response: Response,
    limit: usize,
    target: &ModelTarget,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, InferenceError> {
    let mut source = response.bytes_stream();
    let mut bytes = Vec::new();
    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => return Err(InferenceError::cancelled(&target.id)),
            next = source.next() => next,
        };
        match next {
            Some(Ok(chunk)) => {
                if bytes.len().saturating_add(chunk.len()) > limit {
                    return Err(InferenceError::new(
                        ErrorClass::Protocol,
                        &target.id,
                        "read_body",
                        format!("response body exceeds {limit} byte limit"),
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            Some(Err(error)) => return Err(transport_error(target, error)),
            None => return Ok(bytes),
        }
    }
}

async fn upstream_error(
    target: &ModelTarget,
    response: Response,
    cancel: &CancellationToken,
) -> InferenceError {
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let retry_after_ms = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| seconds * 1_000);
    let bytes = match read_bounded(response, MAX_ERROR_BODY, target, cancel).await {
        Ok(bytes) => bytes,
        Err(error) if error.class == ErrorClass::Cancelled => return error,
        Err(_) => Vec::new(),
    };
    let message = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .map(crate::redaction::sanitize_diagnostic)
        })
        .unwrap_or_else(|| {
            status
                .canonical_reason()
                .unwrap_or("upstream request failed")
                .to_string()
        });
    let class = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ErrorClass::Authentication,
        StatusCode::TOO_MANY_REQUESTS => ErrorClass::RateLimit,
        _ => ErrorClass::Upstream,
    };
    let mut error = InferenceError::new(class, &target.id, "http", message);
    error.status = Some(status.as_u16());
    error.request_id = request_id;
    error.retry_after_ms = retry_after_ms;
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_supports_comments_multiline_and_fragmented_lines() {
        let mut decoder = SseDecoder::new("fixture".into());
        assert!(
            decoder
                .push(b": keepalive\ndata: {\"a\":")
                .unwrap()
                .is_empty()
        );
        let messages = decoder.push(b"1}\ndata: second\n\n").unwrap();
        assert_eq!(
            messages,
            vec![SseMessage {
                data: "{\"a\":1}\nsecond".into(),
                done: false
            }]
        );
    }

    #[test]
    fn done_is_protocol_data_not_eof_guessing() {
        let mut decoder = SseDecoder::new("fixture".into());
        let messages = decoder.push(b"data: [DONE]\n\n").unwrap();
        assert!(messages[0].done);
    }

    #[test]
    fn malformed_sse_utf8_is_a_typed_error() {
        let mut decoder = SseDecoder::new("fixture".into());
        let error = decoder
            .push(&[b'd', b'a', b't', b'a', b':', 0xff, b'\n'])
            .unwrap_err();
        assert_eq!(error.class, ErrorClass::Sse);
    }
}
