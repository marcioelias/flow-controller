# ==========================================
# ESTÁGIO 1: BUILD DA SPA (Vue/Vite)
# O dist/ é embutido no binário Rust via rust-embed (task 16.3)
# ==========================================
FROM node:20-slim AS ui

WORKDIR /ui
COPY VERSION /VERSION
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

# ==========================================
# ESTÁGIO 2: COMPILAÇÃO CARGO
# ==========================================
FROM rust:bookworm AS builder

WORKDIR /app
COPY . .
# SPA construída no estágio anterior — vira bytes do executável
COPY --from=ui /ui/dist ./frontend/dist

RUN cargo build --release

# ==========================================
# ESTÁGIO 3: EXECUÇÃO LEVE (SLIM)
# ==========================================
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates sqlite3 libsqlite3-0 wget && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/collector-core /usr/local/bin/collector-core

# NetFlow/IPFIX ingest + UI/API (a SPA é servida pelo próprio binário)
EXPOSE 2055/udp
EXPOSE 3000

CMD ["collector-core"]
