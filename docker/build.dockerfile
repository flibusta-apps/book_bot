FROM rust:bookworm AS builder

WORKDIR /app

# Copy only dependency manifests first to cache deps layer
COPY Cargo.toml Cargo.lock ./
COPY book_bot/Cargo.toml book_bot/Cargo.toml
COPY book_bot_macros/Cargo.toml book_bot_macros/Cargo.toml

# Create dummy source files so cargo can resolve the workspace
RUN mkdir -p book_bot/src && echo "fn main() {}" > book_bot/src/main.rs
RUN mkdir -p book_bot_macros/src && echo "" > book_bot_macros/src/lib.rs

# Build only dependencies (this layer is cached unless Cargo.toml/Cargo.lock change)
RUN cargo build --release --bin book_bot || true

# Now copy real source code
COPY . .

# Touch source files to ensure they get rebuilt (not the cached dummy)
RUN touch book_bot/src/main.rs book_bot_macros/src/lib.rs

RUN cargo build --release --bin book_bot

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y openssl ca-certificates curl jq \
    && rm -rf /var/lib/apt/lists/*

COPY ./scripts /

RUN chmod +x /start.sh

COPY --from=builder /app/target/release/book_bot /usr/local/bin/book_bot

CMD ["/start.sh"]