use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{extract::Path, routing::get};

use axum_prometheus::PrometheusMetricLayer;
use reqwest::StatusCode;
use tokio::sync::{watch, Mutex};

use std::{net::SocketAddr, sync::Arc};

use teloxide::types::{Update, UpdateKind};

use tower_http::trace::{self, TraceLayer};

use tracing::log;
use tracing::Level;

use super::{publish_started_bot, BOTS_STATE_LOCK, BOTS_VERSIONS};
use crate::bots_manager::utils::{mask_token, mask_uri_path, truncate_for_log};
use crate::bots_manager::{internal::start_bot, BOTS_DATA, BOTS_ROUTES};
use crate::config;

#[derive(Default)]
struct StartBotLocks(
    std::sync::Mutex<std::collections::HashMap<String, std::sync::Weak<Mutex<()>>>>,
);

impl StartBotLocks {
    fn for_token(&self, token: &str) -> Arc<Mutex<()>> {
        let mut locks = self.0.lock().expect("Bot startup locks poisoned");
        locks.retain(|_, lock| lock.strong_count() > 0);
        let slot = locks.entry(token.to_string()).or_default();
        match slot.upgrade() {
            Some(lock) => lock,
            None => {
                let lock = Arc::new(Mutex::new(()));
                *slot = Arc::downgrade(&lock);
                lock
            }
        }
    }
}

#[derive(Clone)]
struct BotIdMakeSpan;

impl<B> tower_http::trace::MakeSpan<B> for BotIdMakeSpan {
    fn make_span(&mut self, request: &axum::http::Request<B>) -> tracing::Span {
        let masked = mask_uri_path(request.uri().path());
        tracing::info_span!(
            "request",
            method = %request.method(),
            uri = %masked,
            version = ?request.version(),
        )
    }
}

async fn bind_webhook_listener(port: u16) -> std::io::Result<tokio::net::TcpListener> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tokio::net::TcpListener::bind(addr).await
}

pub async fn start_axum_server(
    mut shutdown_rx: watch::Receiver<()>,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    async fn telegram_request(
        State(start_bot_locks): State<Arc<StartBotLocks>>,
        Path(token): Path<String>,
        headers: HeaderMap,
        input: String,
    ) -> impl IntoResponse {
        let expected_secret = config::CONFIG.webhook_secret_token.as_str();
        let provided_secret = headers
            .get("x-telegram-bot-api-secret-token")
            .and_then(|v| v.to_str().ok());
        if expected_secret.is_empty() || provided_secret != Some(expected_secret) {
            metrics::counter!("webhook_secret_rejected_total").increment(1u64);
            return StatusCode::FORBIDDEN;
        }

        if !BOTS_DATA.contains_key(&token) {
            return StatusCode::NOT_FOUND;
        }

        let (_, stop_flag, r_tx, _dispatcher_handle) = match BOTS_ROUTES.get(&token).await {
            Some(tx) => tx,
            None => {
                'creator: {
                    let start_bot_mutex = start_bot_locks.for_token(&token);
                    let _guard = start_bot_mutex.lock().await;

                    if BOTS_ROUTES.contains_key(&token) {
                        break 'creator;
                    }

                    let (bot_data, version) = {
                        let _state_guard = BOTS_STATE_LOCK.lock().await;
                        match (BOTS_DATA.get(&token).await, BOTS_VERSIONS.get(&token).await) {
                            (Some(data), Some(version)) => (data, version),
                            _ => return StatusCode::NOT_FOUND,
                        }
                    };
                    let route = start_bot(&bot_data, version).await;
                    if !publish_started_bot(token.clone(), version, route).await {
                        return StatusCode::NOT_FOUND;
                    }
                }

                match BOTS_ROUTES.get(&token).await {
                    None => {
                        log::error!("Cannot get a bot with token: {}", mask_token(&token));
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                    Some(v) => v,
                }
            }
        };

        let tx = match r_tx.get() {
            None => {
                BOTS_ROUTES.remove(&token).await;
                return StatusCode::SERVICE_UNAVAILABLE;
            }
            Some(v) => v,
        };

        if stop_flag.is_stopped() {
            BOTS_ROUTES.remove(&token).await;
            return StatusCode::SERVICE_UNAVAILABLE;
        }

        match serde_json::from_str::<Update>(&input) {
            Ok(update) => {
                // teloxide-core parses updates it doesn't recognize into
                // `UpdateKind::Error(Value::default())`, discarding the raw
                // payload in the process. Re-parse the same input as a bare
                // `Value` here so downstream handlers still see the original
                // update body instead of an empty default.
                if matches!(update.kind, UpdateKind::Error(_)) {
                    // Unknown update types are dropped at the boundary; log
                    // only the id and top-level key names (no payload).
                    let kind = serde_json::from_str::<serde_json::Value>(&input)
                        .ok()
                        .and_then(|v| {
                            v.as_object().map(|o| {
                                o.keys()
                                    .filter(|k| k.as_str() != "update_id")
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(",")
                            })
                        })
                        .unwrap_or_default();
                    log::debug!("Dropping unsupported update id={} type={kind}", update.id.0);
                    return StatusCode::OK;
                }

                // Serialize this cheap membership check + nonblocking send with
                // manager removal; never dispatch via a leftover stale route.
                let _state_guard = BOTS_STATE_LOCK.lock().await;
                if !BOTS_DATA.contains_key(&token) {
                    return StatusCode::NOT_FOUND;
                }
                if stop_flag.is_stopped() {
                    return StatusCode::SERVICE_UNAVAILABLE;
                }
                match tx.try_send(Ok(update)) {
                    Ok(()) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        log::warn!(
                            "Update queue full for Bot(token={}); asking Telegram to retry",
                            mask_token(&token)
                        );
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        log::error!(
                            "Update channel closed for Bot(token={})",
                            mask_token(&token)
                        );
                        BOTS_ROUTES.remove(&token).await;
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                }
            }
            Err(error) => {
                // `error`'s Display can itself embed a fragment of the
                // offending field's raw value on a type-mismatch (e.g.
                // serde_json renders `invalid type: string "<value>",
                // expected i64` — verified empirically). Since this log is
                // ERROR-level and therefore Sentry-bound (main.rs's
                // event_filter), only log the content-free error category
                // here; the full error text and payload go to DEBUG only.
                log::error!(
                    "Failed to parse incoming Telegram update: {:?}",
                    error.classify()
                );
                log::debug!(
                    "Parse error detail: {error}\nMalformed update payload: {}",
                    truncate_for_log(&input, 2000)
                );
            }
        };

        StatusCode::OK
    }

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let start_bot_locks = Arc::new(StartBotLocks::default());

    let app_router = axum::Router::new()
        .route("/{token}/", post(telegram_request))
        .with_state(start_bot_locks)
        .layer(prometheus_layer);

    let metric_router = axum::Router::new()
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .route("/health", get(|| async { StatusCode::OK }));

    let router = axum::Router::new()
        .merge(app_router)
        .merge(metric_router)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(BotIdMakeSpan)
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        );

    let listener = bind_webhook_listener(config::CONFIG.webhook_port).await?;
    log::info!(
        "Webhook server listening on port {}",
        config::CONFIG.webhook_port
    );

    let handle = tokio::spawn(async move {
        log::info!("Start webserver...");

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            })
            .await
            .unwrap();

        log::info!("Webserver shutdown...");
    });

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn startup_is_single_flight_per_token_not_across_tokens() {
        let locks = StartBotLocks::default();
        let first = locks.for_token("first");
        let same = locks.for_token("first");
        let other = locks.for_token("other");
        assert!(Arc::ptr_eq(&first, &same));
        let _guard = first.lock().await;
        assert!(same.try_lock().is_err());
        assert!(other.try_lock().is_ok());
    }

    #[tokio::test]
    async fn bind_webhook_listener_fails_when_port_already_taken() {
        let first = bind_webhook_listener(0).await.unwrap();
        let port = first.local_addr().unwrap().port();

        let second = bind_webhook_listener(port).await;

        assert!(second.is_err());
    }
}
