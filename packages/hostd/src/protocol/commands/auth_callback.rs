use piko_llmd::providers::BrowserAuthInfo;
use piko_protocol::AuthFailureReason;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

type CallbackError = (AuthFailureReason, String);

enum CallbackAttempt {
    Code(String),
    Ignore,
    Terminal(CallbackError),
}

pub(super) async fn receive_browser_callback(
    listener: TcpListener,
    info: &BrowserAuthInfo,
) -> Result<String, CallbackError> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|error| {
            (
                AuthFailureReason::Callback,
                format!("OAuth callback failed: {error}"),
            )
        })?;
        match read_attempt(&mut stream, info).await {
            CallbackAttempt::Code(code) => {
                respond(&mut stream, true).await;
                return Ok(code);
            }
            CallbackAttempt::Ignore => respond(&mut stream, false).await,
            CallbackAttempt::Terminal(error) => {
                respond(&mut stream, false).await;
                return Err(error);
            }
        }
    }
}

async fn read_attempt(stream: &mut TcpStream, info: &BrowserAuthInfo) -> CallbackAttempt {
    let request_bytes = match read_headers(stream).await {
        Ok(bytes) => bytes,
        Err(_) => return CallbackAttempt::Ignore,
    };
    let request = String::from_utf8_lossy(&request_bytes);
    let Some(mut request_line) = request.lines().next().map(str::split_whitespace) else {
        return CallbackAttempt::Ignore;
    };
    if request_line.next() != Some("GET") {
        return CallbackAttempt::Ignore;
    }
    let Some(target) = request_line.next() else {
        return CallbackAttempt::Ignore;
    };
    let Ok(url) = reqwest::Url::parse(&format!("http://localhost{target}")) else {
        return CallbackAttempt::Ignore;
    };
    if url.path() != "/auth/callback" {
        return CallbackAttempt::Ignore;
    }
    let params: std::collections::HashMap<String, String> =
        url.query_pairs().into_owned().collect();
    if params.get("state") != Some(&info.state) {
        return CallbackAttempt::Ignore;
    }
    if let Some(error) = params.get("error") {
        return CallbackAttempt::Terminal((
            AuthFailureReason::Denied,
            params
                .get("error_description")
                .cloned()
                .unwrap_or_else(|| error.clone()),
        ));
    }
    match params.get("code").filter(|code| !code.is_empty()) {
        Some(code) => CallbackAttempt::Code(code.clone()),
        None => CallbackAttempt::Terminal((
            AuthFailureReason::Callback,
            "OAuth callback omitted the authorization code".into(),
        )),
    }
}

async fn read_headers(stream: &mut TcpStream) -> Result<Vec<u8>, ()> {
    let mut request = Vec::with_capacity(1024);
    loop {
        let mut chunk = [0_u8; 1024];
        let read = stream.read(&mut chunk).await.map_err(|_| ())?;
        if read == 0 {
            return Err(());
        }
        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(request);
        }
        if request.len() > 16 * 1024 {
            return Err(());
        }
    }
}

async fn respond(stream: &mut TcpStream, success: bool) {
    let (status, body) = if success {
        ("200 OK", "Sign-in received. You can return to piko.")
    } else {
        (
            "400 Bad Request",
            "Sign-in could not be completed. Return to piko for details.",
        )
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}
