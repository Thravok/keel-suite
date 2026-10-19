//! Localhost WS handshake checks (Origin + Bearer token).

use crate::{ORIGIN_KEEL, ORIGIN_STREAMDECK};
use std::fs;
use std::path::Path;
use thiserror::Error;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::{Response as HttpResponse, StatusCode};

/// Filename written under each engine's Application Support directory.
pub const RPC_TOKEN_FILENAME: &str = "rpc.token";

/// HTTP Authorization scheme prefix.
pub const BEARER_PREFIX: &str = "Bearer ";

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("empty rpc token")]
    Empty,
}

/// Whether `Origin` is an allowed first-party suite / Stream Deck plugin origin.
pub fn origin_allowed(origin: &str) -> bool {
    origin.starts_with(ORIGIN_KEEL) || origin.starts_with(ORIGIN_STREAMDECK)
}

/// Generate a 256-bit hex session token (new each engine start).
pub fn generate_rpc_token() -> String {
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    format!("{}{}", a.simple(), b.simple())
}

/// Write token to `path` with mode `0600` on Unix.
pub fn write_rpc_token(path: &Path, token: &str) -> Result<(), TokenError> {
    if token.is_empty() {
        return Err(TokenError::Empty);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(token.as_bytes())?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        fs::write(path, token.as_bytes())?;
    }
    Ok(())
}

/// Read and trim a token file previously written by [`write_rpc_token`].
pub fn read_rpc_token(path: &Path) -> Result<String, TokenError> {
    let raw = fs::read_to_string(path)?;
    let token = raw.trim().to_string();
    if token.is_empty() {
        return Err(TokenError::Empty);
    }
    Ok(token)
}

/// Format `Authorization: Bearer <token>` header value.
pub fn bearer_header_value(token: &str) -> String {
    format!("{BEARER_PREFIX}{token}")
}

fn bearer_from_request(req: &Request) -> Option<&str> {
    let value = req
        .headers()
        .get("Authorization")
        .or_else(|| req.headers().get("authorization"))
        .and_then(|v| v.to_str().ok())?;
    value.strip_prefix(BEARER_PREFIX)
}

fn tokens_equal(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes()
        .iter()
        .zip(b.as_bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn reject(status: StatusCode, message: &str) -> ErrorResponse {
    HttpResponse::builder()
        .status(status)
        .body(Some(message.to_string()))
        .unwrap_or_else(|_| {
            HttpResponse::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Some("handshake error".into()))
                .expect("static error response")
        })
}

/// Validate Origin + Bearer for a suite RPC upgrade; reject with 401/403 otherwise.
#[allow(clippy::result_large_err)] // tungstenite ErrorResponse is inherently large
pub fn authorize_ws_upgrade(
    req: &Request,
    response: Response,
    expected_token: &str,
) -> Result<Response, ErrorResponse> {
    let origin = req
        .headers()
        .get("Origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !origin_allowed(origin) {
        return Err(reject(StatusCode::FORBIDDEN, "forbidden origin"));
    }
    let Some(presented) = bearer_from_request(req) else {
        return Err(reject(StatusCode::UNAUTHORIZED, "missing bearer token"));
    };
    if !tokens_equal(presented, expected_token) {
        return Err(reject(StatusCode::UNAUTHORIZED, "invalid bearer token"));
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_rules() {
        assert!(origin_allowed("keel://"));
        assert!(origin_allowed("keel://deck"));
        assert!(origin_allowed("streamdeck://"));
        assert!(!origin_allowed(""));
        assert!(!origin_allowed("null"));
        assert!(!origin_allowed("https://evil.example"));
    }

    #[test]
    fn token_roundtrip() {
        let dir = std::env::temp_dir().join(format!("keel-rpc-token-{}", uuid::Uuid::new_v4()));
        let path = dir.join(RPC_TOKEN_FILENAME);
        let token = generate_rpc_token();
        write_rpc_token(&path, &token).unwrap();
        assert_eq!(read_rpc_token(&path).unwrap(), token);
        let _ = fs::remove_dir_all(dir);
    }
}
