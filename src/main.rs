mod config;
mod cookie;
mod sandbox;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Context;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::RwLock;
use tokio_tungstenite::connect_async;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::{error, info, warn};

use crate::{
    config::Config,
    cookie::{cookie_header, find_cookie, new_session_id},
    sandbox::{ws_request, SandboxManager, SandboxRecord},
};

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    sandboxes: Arc<RwLock<std::collections::HashMap<String, SandboxRecord>>>,
    manager: Arc<SandboxManager>,
}

#[derive(Serialize)]
struct SessionResponse {
    session_id: String,
    sandbox: Option<SandboxRecord>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Arc::new(Config::parse().normalized()?);
    let manager = Arc::new(SandboxManager::new(config.clone()));
    let bind = config
        .bind_addr
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid bind address {}", config.bind_addr))?;

    let state = AppState {
        config: config.clone(),
        sandboxes: Arc::new(RwLock::new(std::collections::HashMap::new())),
        manager,
    };

    let public_dir = PathBuf::from(&config.public_dir);
    let app = Router::new()
        .route("/api/session", get(session))
        .route("/ws/codex", get(ws_codex))
        .fallback_service(ServeDir::new(public_dir).append_index_html_on_directories(true))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    info!(%bind, provider = %config.sandbox_provider, "starting nju-cli web service");
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (session_id, set_cookie) = ensure_session(&state.config, &headers);
    let sandbox = state.sandboxes.read().await.get(&session_id).cloned();

    let mut response = Json(SessionResponse {
        session_id,
        sandbox,
    })
    .into_response();

    if let Some(value) = set_cookie {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

async fn ws_codex(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let (session_id, set_cookie) = ensure_session(&state.config, &headers);
    let response = ws.on_upgrade(move |socket| proxy_codex(socket, state, session_id));

    if let Some(value) = set_cookie {
        let mut response = response.into_response();
        response.headers_mut().insert(header::SET_COOKIE, value);
        response
    } else {
        response.into_response()
    }
}

fn ensure_session(config: &Config, headers: &HeaderMap) -> (String, Option<HeaderValue>) {
    if let Some(id) = find_cookie(headers, &config.session_cookie) {
        return (id, None);
    }

    let id = new_session_id();
    let header = cookie_header(&config.session_cookie, &id, config.session_max_age_seconds);
    (id, HeaderValue::from_str(&header).ok())
}

async fn proxy_codex(socket: WebSocket, state: AppState, session_id: String) {
    let sandbox = match get_or_create_sandbox(&state, &session_id).await {
        Ok(sandbox) => sandbox,
        Err(err) => {
            error!(%session_id, error = %format_error_chain(&err), "failed to prepare sandbox");
            send_error_and_close(socket, StatusCode::SERVICE_UNAVAILABLE, &err.to_string()).await;
            return;
        }
    };

    info!(%session_id, endpoint = %sandbox.codex_ws_url, "connecting browser to codex app-server");
    let request = match state
        .manager
        .codex_ws_auth_token(&session_id)
        .and_then(|token| ws_request(&sandbox.codex_ws_url, &token))
    {
        Ok(request) => request,
        Err(err) => {
            error!(%session_id, error = %format_error_chain(&err), "failed to build codex app-server request");
            send_error_and_close(socket, StatusCode::BAD_GATEWAY, &err.to_string()).await;
            return;
        }
    };
    let upstream = match connect_async(request).await {
        Ok((stream, _)) => stream,
        Err(err) => {
            error!(%session_id, error = %err, "failed to connect to codex app-server");
            send_error_and_close(socket, StatusCode::BAD_GATEWAY, &err.to_string()).await;
            return;
        }
    };

    let (mut client_tx, mut client_rx) = socket.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();

    let client_to_upstream = async {
        while let Some(msg) = client_rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Text(
                            text.to_string().into(),
                        ))
                        .await?;
                }
                Ok(Message::Binary(bytes)) => {
                    upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            bytes.into(),
                        ))
                        .await?;
                }
                Ok(Message::Ping(bytes)) => {
                    upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Ping(bytes.into()))
                        .await?;
                }
                Ok(Message::Pong(bytes)) => {
                    upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Pong(bytes.into()))
                        .await?;
                }
                Ok(Message::Close(frame)) => {
                    let frame =
                        frame.map(|f| tokio_tungstenite::tungstenite::protocol::CloseFrame {
                            code: f.code.into(),
                            reason: f.reason.to_string().into(),
                        });
                    let _ = upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Close(frame))
                        .await;
                    break;
                }
                Err(err) => return Err(anyhow::anyhow!(err)),
            }
        }
        Ok::<_, anyhow::Error>(())
    };

    let upstream_to_client = async {
        while let Some(msg) = upstream_rx.next().await {
            match msg? {
                tokio_tungstenite::tungstenite::Message::Text(text) => {
                    client_tx
                        .send(Message::Text(text.to_string().into()))
                        .await?;
                }
                tokio_tungstenite::tungstenite::Message::Binary(bytes) => {
                    client_tx
                        .send(Message::Binary(bytes.to_vec().into()))
                        .await?;
                }
                tokio_tungstenite::tungstenite::Message::Ping(bytes) => {
                    client_tx.send(Message::Ping(bytes.to_vec().into())).await?;
                }
                tokio_tungstenite::tungstenite::Message::Pong(bytes) => {
                    client_tx.send(Message::Pong(bytes.to_vec().into())).await?;
                }
                tokio_tungstenite::tungstenite::Message::Close(frame) => {
                    let frame = frame.map(|f| axum::extract::ws::CloseFrame {
                        code: f.code.into(),
                        reason: f.reason.to_string().into(),
                    });
                    let _ = client_tx.send(Message::Close(frame)).await;
                    break;
                }
                tokio_tungstenite::tungstenite::Message::Frame(_) => {}
            }
        }
        Ok::<_, anyhow::Error>(())
    };

    tokio::select! {
        result = client_to_upstream => {
            if let Err(err) = result {
                warn!(%session_id, error = %err, "client to upstream proxy stopped with error");
            }
        }
        result = upstream_to_client => {
            if let Err(err) = result {
                warn!(%session_id, error = %err, "upstream to client proxy stopped with error");
            }
        }
    }
}

fn format_error_chain(err: &anyhow::Error) -> String {
    err.chain()
        .enumerate()
        .map(|(index, cause)| format!("{index}: {cause}"))
        .collect::<Vec<_>>()
        .join("; ")
}

async fn get_or_create_sandbox(
    state: &AppState,
    session_id: &str,
) -> anyhow::Result<SandboxRecord> {
    if let Some(existing) = state.sandboxes.read().await.get(session_id).cloned() {
        return Ok(existing);
    }

    let mut write = state.sandboxes.write().await;
    if let Some(existing) = write.get(session_id).cloned() {
        return Ok(existing);
    }

    let record = state.manager.ensure(session_id).await?;
    write.insert(session_id.to_owned(), record.clone());
    Ok(record)
}

async fn send_error_and_close(mut socket: WebSocket, code: StatusCode, message: &str) {
    let payload = serde_json::json!({
        "type": "error",
        "status": code.as_u16(),
        "message": message,
    });
    let _ = socket.send(Message::Text(payload.to_string().into())).await;
    let _ = socket.close().await;
}
