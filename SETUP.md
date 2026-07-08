# Setup do Ambiente de Desenvolvimento

## 📋 Pré-requisitos

### Sistema
- Linux (Ubuntu 20.04+, Fedora 35+, Arch)
- Docker & Docker Compose
- Rust (última versão estável)
- Node.js 20+

### Dependências do Sistema

**Ubuntu/Debian/Pop!_OS:**
```bash
sudo apt-get update
sudo apt-get install -y libssl-dev pkg-config build-essential libsqlite3-dev
```

**Fedora/RHEL/CentOS:**
```bash
sudo dnf install -y openssl-devel pkg-config gcc sqlite-devel
```

**Arch/Manjaro:**
```bash
sudo pacman -S --needed openssl pkg-config base-devel sqlite
```

**Ou use o script automático:**
```bash
./install-deps.sh
```

## 🚀 Setup Rápido

```bash
# 1. Instalar dependências do sistema
./install-deps.sh

# 2. Instalar dependências do frontend
cd frontend && npm install && cd ..

# 3. Validar que tudo compila
./quick-check.sh

# 4. Buildar e subir containers
docker compose up --build -d

# 5. Resetar banco (primeira vez)
./reset-clickhouse.sh

# 6. Acessar
http://localhost:8080
Login: admin / admin123
```

## 🛠️ Desenvolvimento Local (sem Docker)

### Backend
```bash
# Compilar
cargo build

# Rodar (precisa do ClickHouse rodando)
export CLICKHOUSE_URL=http://localhost:8123
cargo run
```

### Frontend
```bash
cd frontend

# Dev server
npm run dev  # Abre em http://localhost:5173

# Build
npm run build
```

### ClickHouse local
```bash
docker run -d --name clickhouse \
  -p 8123:8123 \
  -p 9000:9000 \
  clickhouse/clickhouse-server:23.8
```

## 🧪 Testando Localmente

```bash
# Testes backend
cargo test

# Testes frontend
cd frontend && npm test

# Ou tudo de uma vez
make test
```

## 🐛 Problemas Comuns

### "Could not find openssl"
**Solução:** Execute `./install-deps.sh`

### "libsqlite3 not found"
**Solução:** Execute `./install-deps.sh`

### "npm: command not found"
**Solução:** Instale Node.js 20+ via nvm ou gerenciador de pacotes

### "docker: permission denied"
**Problema:** Usuário não tem permissão para usar Docker

**Solução Permanente:**
```bash
# Adicionar usuário ao grupo docker
./fix-docker-permissions.sh

# Depois fazer logout/login OU executar:
newgrp docker
```

**Solução Temporária (usar sudo):**
```bash
# Usar o wrapper (detecta automaticamente)
./docker-wrapper.sh build
./docker-wrapper.sh up

# OU usar sudo diretamente
sudo docker compose build
sudo docker compose up -d
```

### "Port 8123 already in use"
**Solução:** Você tem ClickHouse rodando localmente
```bash
# Parar ClickHouse local
docker stop clickhouse
# OU usar outro container
docker compose down && docker compose up -d
```

## 📦 Estrutura do Projeto

```
.
├── collector-core/     # Backend Rust (NetFlow collector)
├── frontend/          # Frontend Vue 3 + TypeScript
├── clickhouse-exporter/ # ClickHouse client
├── netflow-parser/    # NetFlow/IPFIX parser
├── aggregator/        # Flow aggregation
├── flow-types/        # Tipos compartilhados
├── template-cache/    # Template cache
├── metrics/           # Prometheus metrics
└── docker-compose.yml # Orquestração
```

## 🎯 Próximos Passos

Depois do setup, veja:
- `WORKFLOW.md` - Workflow de desenvolvimento
- `TESTING.md` - Como rodar testes
- `Makefile` - Comandos disponíveis
