# ==========================================
# ESTÁGIO 1: COMPILAÇÃO CARGO 
# ==========================================
FROM rust:bookworm as builder

WORKDIR /app
COPY . .

# Faz o build absoluto em modo de extrema performance release
RUN cargo build --release

# ==========================================
# ESTÁGIO 2: EXECUÇÃO LEVE (SLIM)
# ==========================================
FROM debian:bookworm-slim

# Variáveis globais necessárias
WORKDIR /app

# Instala SSL genérico caso precise de comunicação e SQLite para o banco de autenticação
RUN apt-get update && apt-get install -y ca-certificates sqlite3 libsqlite3-0 wget && rm -rf /var/lib/apt/lists/*

# Extraimos só o binário duro sem cache de deps de 5GB do target e jogamos pra imagem leve
COPY --from=builder /app/target/release/collector-core /usr/local/bin/collector-core

# O Ingress UDP que captura os espelhos mikrotiks
EXPOSE 2055/udp

# Acorda o monstrinho
CMD ["collector-core"]
