use axum::extract::Path;
use axum::response::IntoResponse;
use axum::routing::post;

use reqwest::StatusCode;

use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use teloxide::types::{Update, UpdateKind};

use tower_http::trace::TraceLayer;

use tracing::log;

use crate::bots_manager::{internal::start_bot, BOTS_DATA, BOTS_ROUTES, SERVER_PORT};

pub async fn start_axum_server(stop_signal: Arc<AtomicBool>) {
    async fn telegram_request(Path(token): Path<String>, input: String) -> impl IntoResponse {
        let (_, r_tx) = match BOTS_ROUTES.get(&token).await {
            Some(tx) => tx,
            None => {
                let bot_data = BOTS_DATA.get(&token).await;

                if bot_data.is_none() {
                    return StatusCode::NOT_FOUND;
                }

                let start_result = start_bot(&bot_data.unwrap(), SERVER_PORT).await;

                if !start_result {
                    return StatusCode::SERVICE_UNAVAILABLE;
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

    let router = axum::Router::new()
        .route("/:token/", post(telegram_request))
        .layer(TraceLayer::new_for_http());

    tokio::spawn(async move {
        log::info!("Start webserver...");

        let addr = SocketAddr::from(([0, 0, 0, 0], SERVER_PORT));

        axum::Server::bind(&addr)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async move {
                loop {
                    if !stop_signal.load(Ordering::SeqCst) {
                        break;
                    };
                }
            })
            .await
            .expect("Axum server error");

        log::info!("Webserver shutdown...");
    });
}