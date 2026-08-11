use super::*;
use piko_llmd::providers::BrowserAuthInfo;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

fn browser_info(state: &str) -> BrowserAuthInfo {
    BrowserAuthInfo {
        authorization_url: "https://auth.example/login".into(),
        redirect_uri: "http://localhost/auth/callback".into(),
        state: state.into(),
        code_verifier: "verifier".into(),
        expires_in_seconds: 60,
    }
}

#[tokio::test]
async fn callback_accepts_matching_state_and_returns_code() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(
            async move { receive_browser_callback(listener, &browser_info("expected")).await },
        );
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            b"GET /auth/callback?code=code-123&state=expected HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )
        .await
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap(), "code-123");
}

#[tokio::test]
async fn callback_ignores_mismatched_state_then_accepts_valid_callback() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task =
        tokio::spawn(
            async move { receive_browser_callback(listener, &browser_info("expected")).await },
        );
    let mut wrong = TcpStream::connect(addr).await.unwrap();
    wrong
        .write_all(
            b"GET /auth/callback?code=code-123&state=wrong HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )
        .await
        .unwrap();
    let mut valid = TcpStream::connect(addr).await.unwrap();
    valid
        .write_all(
            b"GET /auth/callback?code=code-456&state=expected HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )
        .await
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap(), "code-456");
}
