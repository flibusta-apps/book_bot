use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{extract::Path, routing::get};

use axum_prometheus::PrometheusMetricLayer;
use reqwest::StatusCode;
use tokio::sync::Mutex;
use tokio::time;

use std::time::Duration;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use teloxide::types::{Update, UpdateKind};

use tower_http::trace::{self, TraceLayer};

use tracing::log;
use tracing::Level;

use crate::bots_manager::{internal::start_bot, BOTS_DATA, BOTS_ROUTES, SERVER_PORT};

pub async fn start_axum_server(stop_signal: Arc<AtomicBool>) {
    async fn telegram_request(
        State(start_bot_mutex): State<Arc<Mutex<()>>>,
        Path(token): Path<String>,
        input: String,
    ) -> impl IntoResponse {
        let (_, r_tx) = match BOTS_ROUTES.get(&token).await {
            Some(tx) => tx,
            None => {
                let bot_data = BOTS_DATA.get(&token).await;

                if bot_data.is_none() {
                    return StatusCode::NOT_FOUND;
                }

                'creator: {
                    let _guard = start_bot_mutex.lock().await;

                    if BOTS_ROUTES.contains_key(&token) {
                        break 'creator;
                    }

                    start_bot(&bot_data.unwrap()).await
                }

                BOTS_ROUTES.get(&token).await.unwrap()
            }
        };

        let tx = match r_tx.get() {
            Some(v) => v,
            None => {
                BOTS_ROUTES.remove(&token).await;
                return StatusCode::SERVICE_UNAVAILABLE;
            }
        };

        match serde_json::from_str::<Update>(&input) {
            Ok(mut update) => {
                if let UpdateKind::Error(value) = &mut update.kind {
                    *value = serde_json::from_str(&input).unwrap_or_default();
                }

                if let Err(err) = tx.send(Ok(update)) {
                    log::error!("{:?}", err);
                    BOTS_ROUTES.remove(&token).await;
                    return StatusCode::SERVICE_UNAVAILABLE;
                }
            }
            Err(error) => {
                log::error!(
                    "Cannot parse an update.\nError: {:?}\nValue: {}\n\
                     This is a bug in teloxide-core, please open an issue here: \
                     https://github.com/teloxide/teloxide/issues.",
                    error,
                    input
                );
            }
        };

        StatusCode::OK
    }

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let start_bot_mutex = Arc::new(Mutex::new(()));

    let app_router = axum::Router::new()
        .route("/:token/", post(telegram_request))
        .with_state(start_bot_mutex)
        .layer(prometheus_layer);

    let metric_router =
        axum::Router::new().route("/metrics", get(|| async move { metric_handle.render() }));

    let router = axum::Router::new()
        .nest("/", app_router)
        .nest("/", metric_router)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        );

    tokio::spawn(async move {
        log::info!("Start webserver...");

        let addr = SocketAddr::from(([0, 0, 0, 0], SERVER_PORT));

        axum::Server::bind(&addr)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async move {
                let mut interval = time::interval(Duration::from_secs(1));

                loop {
                    if !stop_signal.load(Ordering::SeqCst) {
                        break;
                    };

                    interval.tick().await;
                }
            })
            .await
            .expect("Axum server error");

        log::info!("Webserver shutdown...");
    });
}
