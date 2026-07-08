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

use crate::bots_manager::utils::{mask_token, mask_uri_path};
use crate::bots_manager::{internal::start_bot, BOTS_DATA, BOTS_ROUTES};
use crate::config;

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
        State(start_bot_mutex): State<Arc<Mutex<()>>>,
        Path(token): Path<String>,
        headers: HeaderMap,
        input: String,
    ) -> impl IntoResponse {
        let expected_secret = config::CONFIG.webhook_secret_token.as_str();
        let provided_secret = headers
            .get("x-telegram-bot-api-secret-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided_secret != expected_secret {
            metrics::counter!("webhook_secret_rejected_total").increment(1u64);
            return StatusCode::FORBIDDEN;
        }

        let (_, stop_flag, r_tx, _dispatcher_handle) = match BOTS_ROUTES.get(&token).await {
            Some(tx) => tx,
            None => {
                let bot_data = match BOTS_DATA.get(&token).await {
                    Some(v) => v,
                    None => {
                        return StatusCode::NOT_FOUND;
                    }
                };

                'creator: {
                    let _guard = start_bot_mutex.lock().await;

                    if BOTS_ROUTES.contains_key(&token) {
                        break 'creator;
                    }

                    start_bot(&bot_data).await
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
            Ok(mut update) => {
                if let UpdateKind::Error(value) = &mut update.kind {
                    *value = serde_json::from_str(&input).unwrap_or_default();
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
                log::error!(
                    "Cannot parse an update.\nError: {error:?}\nValue: {input}\n\
                     This is a bug in teloxide-core, please open an issue here: \
                     https://github.com/teloxide/teloxide/issues."
                );
            }
        };

        StatusCode::OK
    }

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let start_bot_mutex = Arc::new(Mutex::new(()));

    let app_router = axum::Router::new()
        .route("/{token}/", post(telegram_request))
        .with_state(start_bot_mutex)
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
    async fn bind_webhook_listener_fails_when_port_already_taken() {
        let first = bind_webhook_listener(0).await.unwrap();
        let port = first.local_addr().unwrap().port();

        let second = bind_webhook_listener(port).await;

        assert!(second.is_err());
    }
}
