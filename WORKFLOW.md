# Workflow de Desenvolvimento

Este documento descreve o workflow recomendado para trabalhar no projeto.

## 🚀 Quick Start

```bash
# 1. Validar e buildar tudo (RECOMENDADO)
./validate-and-build.sh

# 2. Subir os containers
docker compose up -d

# 3. Acessar o dashboard
http://localhost:8080
Login: admin / admin123
```

## 🧪 Testando Código

### Antes de Commitar
```bash
# Opção 1: Usando Make (recomendado)
make test

# Opção 2: Testando separadamente
cargo test                    # Backend
cd frontend && npm test       # Frontend
```

### Durante Desenvolvimento
```bash
# Backend (modo watch)
cargo watch -x test

# Frontend (modo watch)
cd frontend && npm test
```

## 🏗️ Buildando

### Build Local (sem Docker)
```bash
# Tudo de uma vez
make build

# Separadamente
cargo build --release         # Backend
cd frontend && npm run build  # Frontend
```

### Build Docker
```bash
# Validar + Buildar
./validate-and-build.sh

# Apenas buildar
docker compose build

# Buildar serviço específico
docker compose build flow-collector
docker compose build dashboard
```

## 🐛 Debugging

### Backend não compila
```bash
# Ver erros detalhados
cargo check

# Ver warnings
cargo clippy

# Fix automático
cargo fix --allow-dirty
```

### Frontend não compila
```bash
cd frontend
npm run build  # Ver erros detalhados
```

### Testes falhando
```bash
# Backend - ver output completo
cargo test -- --nocapture

# Frontend - modo UI interativo
cd frontend && npm run test:ui
```

### Container não sobe
```bash
# Ver logs
docker compose logs flow-collector
docker compose logs dashboard

# Restart específico
docker compose restart flow-collector
```

## 📊 ClickHouse

### Resetar tabelas (após mudanças no schema)
```bash
./reset-clickhouse.sh
docker compose restart flow-collector
```

### Ver dados
```bash
# Query direta
docker exec ch-database clickhouse-client --query "SELECT * FROM network_flows_v4 LIMIT 10 FORMAT Pretty"

# Count
docker exec ch-database clickhouse-client --query "SELECT count() FROM network_flows_v4"
```

## 🔧 Comandos Úteis

```bash
# Ver todos os comandos disponíveis
make help

# Logs em tempo real
docker compose logs -f

# Parar tudo
docker compose down

# Limpar volumes (WARNING: deleta dados)
docker compose down -v

# Rebuild completo (limpa cache)
docker compose build --no-cache
```

## 📝 Checklist antes de Commit

```bash
# Quick check (30s)
./quick-check.sh

# OU completo (com testes)
make test
make check-warnings
```

- [ ] `./quick-check.sh` passa
- [ ] `make test` passa
- [ ] `make check-warnings` passa (sem warnings)
- [ ] Testei localmente as mudanças

## 🆘 Problemas Comuns

### "no library targets found"
**Causa:** Tentando rodar `cargo test --lib` em binário  
**Solução:** Use apenas `cargo test`

### "error: unused imports"
**Causa:** Imports não removidos  
**Solução:** `cargo fix --allow-dirty`

### "WebSocket 401 Unauthorized"
**Causa:** nginx.conf com auth_basic no /ws  
**Solução:** Já corrigido, rebuild dashboard

### "IPs invertidos no ClickHouse"
**Causa:** Schema antigo com IPv4 type  
**Solução:** `./reset-clickhouse.sh`

### "Testes frontend falhando"
**Causa:** Dependências não instaladas  
**Solução:** `cd frontend && npm install`

### "Warnings no código"
**Causa:** Imports não usados, variáveis não usadas  
**Solução:** `./fix-warnings.sh` ou `cargo fix --allow-dirty`

## 🧹 Mantendo Código Limpo

```bash
# Verificar warnings
make check-warnings

# Corrigir automaticamente
./fix-warnings.sh

# Linters completos
make lint
```
