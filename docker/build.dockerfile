FROM rust:bookworm AS chef

RUN cargo install cargo-chef --locked

WORKDIR /app

FROM chef AS planner

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json

# Build only dependencies (this layer is cached unless Cargo.toml/Cargo.lock change).
# Unlike the old dummy-main.rs + `|| true` trick, a broken dependency fails here
# with a normal cargo error, and build.rs / new binary targets are covered
# automatically since the recipe captures the whole dependency graph.
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

RUN cargo build --release --bin book_bot

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y openssl ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /usr/sbin/nologin app

COPY --from=builder /app/target/release/book_bot /usr/local/bin/book_bot

USER app

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:${WEBHOOK_PORT}/health || exit 1

CMD ["/usr/local/bin/book_bot"]
