FROM rust:slim-bullseye as builder

RUN apt-get update \
    && apt-get install -y pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/myapp
COPY . .

RUN cargo install --path .


FROM debian:bullseye-slim

RUN apt-get update \
    && apt-get install -y openssl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN update-ca-certificates

COPY --from=builder /usr/local/cargo/bin/book_bot /usr/local/bin/book_bot

CMD book_bot