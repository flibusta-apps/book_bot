FROM rust:bookworm AS builder

WORKDIR /app

COPY . .

RUN cargo build --release --bin book_bot


FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y openssl ca-certificates curl jq \
    && rm -rf /var/lib/apt/lists/*

RUN update-ca-certificates

COPY ./scripts/*.sh /
RUN chmod +x /*.sh

WORKDIR /app

COPY --from=builder /app/target/release/book_bot /usr/local/bin
ENTRYPOINT ["/start.sh"]
