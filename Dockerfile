# ── Stage 1: Build ────────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN cargo build --release -p infolit-web

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/infolit-web /usr/local/bin/infolit-web
COPY content/ /app/content/

WORKDIR /app
EXPOSE 3000

# 可透過環境變數覆寫：OLLAMA_URL, MODEL, CONTENT_DIR, PORT, THINK
ENV CONTENT_DIR=/app/content

CMD ["infolit-web"]
