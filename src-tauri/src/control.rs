//! The control channel: how the app's own server reaches native capabilities.
//!
//! The bridge runs page-to-shell. That is the right shape on mobile, where
//! Hotwire Native has no server to talk to, but a desktop app runs its server
//! in the same process tree — so Ruby can have a channel of its own, and
//! without one a background job cannot raise a notification while no page is
//! open.
//!
//! The shape is deliberately small: a loopback listener on a port the OS picks,
//! one token, one endpoint. Ruby posts a bridge message and gets the reply.
//!
//! ```text
//! POST /invoke
//! X-Desktop-Token: <token>
//! {"component":"notification","event":"show","data":{"title":"Done"}}
//! ```
//!
//! Security rests on three things. The listener binds 127.0.0.1 only. Every
//! request carries a token generated from OS entropy and compared in constant
//! time. And the token reaches the child over stdin rather than the
//! environment or the command line, so another process on the machine cannot
//! read it out of the process list — which is the reason Neutralino does the
//! same thing.

use crate::bridge::BridgeMessage;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Where the control channel is listening, and the secret to reach it.
#[derive(Debug, Clone)]
pub struct ControlChannel {
    pub url: String,
    pub token: String,
}

/// Header the token travels in.
pub const TOKEN_HEADER: &str = "x-desktop-token";

/// Largest request body accepted, so a malformed Content-Length cannot make the
/// shell allocate without bound.
const MAX_BODY: usize = 256 * 1024;

/// A token with real entropy behind it.
///
/// Deliberately not `uuid_simple()`: that is a nanosecond counter, which is fine
/// for a window label and guessable for a credential.
fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS must provide entropy for the control token");
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Compare in constant time, so a wrong token cannot be found a byte at a time.
fn token_matches(expected: &str, given: &str) -> bool {
    if expected.len() != given.len() {
        return false;
    }
    expected
        .bytes()
        .zip(given.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// What a request's head says. Parsed separately from the socket so the
/// security-relevant part — which token, which path — can be tested directly.
#[derive(Debug, PartialEq, Eq)]
pub struct RequestHead {
    pub method: String,
    pub path: String,
    pub token: String,
    pub content_length: usize,
}

fn parse_head(head: &str) -> RequestHead {
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut token = String::new();
    let mut content_length = 0usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        // Header names are case-insensitive, and a client may send any casing.
        match name.trim().to_ascii_lowercase().as_str() {
            TOKEN_HEADER => token = value.trim().to_string(),
            "content-length" => content_length = value.trim().parse().unwrap_or(0),
            _ => {}
        }
    }

    RequestHead {
        method,
        path,
        token,
        content_length,
    }
}

/// Start listening, and report where.
pub async fn start(app: tauri::AppHandle) -> Result<ControlChannel, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Could not open the control channel: {}", e))?;
    let port = listener.local_addr().map_err(|e| format!("{}", e))?.port();

    let token = Arc::new(generate_token());
    let channel = ControlChannel {
        url: format!("http://127.0.0.1:{}", port),
        token: (*token).clone(),
    };

    log::info!("Control channel on {}", channel.url);

    let token_for_loop = token.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let app = app.clone();
                    let token = token_for_loop.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = serve(stream, app, token).await {
                            log::debug!("Control request ended: {}", e);
                        }
                    });
                }
                Err(e) => {
                    log::warn!("Control channel stopped accepting: {}", e);
                    break;
                }
            }
        }
    });

    Ok(channel)
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        status,
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await
}

async fn serve(
    mut stream: TcpStream,
    app: tauri::AppHandle,
    token: Arc<String>,
) -> Result<(), String> {
    // Read until the end of the headers. Small and bounded: this is a loopback
    // control channel, not a general-purpose server.
    let mut buffer = Vec::with_capacity(2048);
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|e| format!("read failed: {}", e))?;
        if read == 0 {
            return Err("client closed before sending a request".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if buffer.len() > MAX_BODY {
            let _ = respond(&mut stream, "431 Request Header Fields Too Large", "{}").await;
            return Err("headers too large".into());
        }
    };

    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let parsed = parse_head(&head);
    let content_length = parsed.content_length;

    if !token_matches(&token, &parsed.token) {
        log::warn!("Control channel refused a request with a bad or missing token");
        let _ = respond(&mut stream, "401 Unauthorized", r#"{"error":"bad token"}"#).await;
        return Ok(());
    }

    if parsed.method != "POST" || parsed.path != "/invoke" {
        let _ = respond(
            &mut stream,
            "404 Not Found",
            r#"{"error":"POST /invoke only"}"#,
        )
        .await;
        return Ok(());
    }

    if content_length > MAX_BODY {
        let _ = respond(&mut stream, "413 Payload Too Large", "{}").await;
        return Ok(());
    }

    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|e| format!("read failed: {}", e))?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }

    let message: BridgeMessage = match serde_json::from_slice(&body) {
        Ok(m) => m,
        Err(e) => {
            let payload = serde_json::json!({ "error": format!("bad message: {}", e) });
            let _ = respond(&mut stream, "400 Bad Request", &payload.to_string()).await;
            return Ok(());
        }
    };

    let (status, payload) = match crate::bridge::dispatch(&app, &message).await {
        Ok(value) => ("200 OK", value),
        Err(e) => (
            "500 Internal Server Error",
            serde_json::json!({ "error": e }),
        ),
    };
    let _ = respond(&mut stream, status, &payload.to_string()).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_is_parsed_whatever_the_header_casing() {
        let head = "POST /invoke HTTP/1.1\r\nHost: x\r\nX-Desktop-Token: abc\r\nContent-Length: 42\r\n\r\n";
        let parsed = parse_head(head);
        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.path, "/invoke");
        assert_eq!(parsed.token, "abc");
        assert_eq!(parsed.content_length, 42);
    }

    #[test]
    fn a_missing_token_parses_as_empty_rather_than_matching() {
        let parsed = parse_head("POST /invoke HTTP/1.1\r\nHost: x\r\n\r\n");
        assert_eq!(parsed.token, "");
        assert!(!token_matches(&generate_token(), &parsed.token));
    }

    #[test]
    fn a_malformed_content_length_does_not_panic_or_allocate() {
        let parsed = parse_head("POST /invoke HTTP/1.1\r\nContent-Length: banana\r\n\r\n");
        assert_eq!(parsed.content_length, 0);
    }

    #[test]
    fn only_post_invoke_is_routed() {
        for (method, path) in [
            ("GET", "/invoke"),
            ("POST", "/"),
            ("POST", "/invoke/../etc"),
        ] {
            let head = format!("{} {} HTTP/1.1\r\n\r\n", method, path);
            let parsed = parse_head(&head);
            assert!(
                parsed.method != "POST" || parsed.path != "/invoke",
                "{} {} should not route to the dispatcher",
                method,
                path
            );
        }
    }

    #[test]
    fn a_token_is_long_and_not_repeated() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64, "32 bytes of entropy, hex encoded");
        assert_ne!(a, b, "two tokens must not collide");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn token_comparison_accepts_only_the_exact_token() {
        let token = "a".repeat(64);
        assert!(token_matches(&token, &token));
        assert!(!token_matches(&token, &"a".repeat(63)));
        assert!(!token_matches(&token, &format!("{}b", "a".repeat(63))));
        assert!(!token_matches(&token, ""));
    }
}
