# Flow Collector

Sistema de alta performance para coleta, agregação e visualização de NetFlow/IPFIX.

## ✨ Features

- 🚀 **Alta Performance** - Processa milhões de flows por segundo
- 📊 **Dashboard em Tempo Real** - Vue 3 + WebSockets
- 🔐 **Autenticação JWT** - Sistema completo com gerenciamento de usuários
- 🎯 **Filtros por Device** - Visualize tráfego por exporter específico
- 💾 **ClickHouse** - Armazenamento otimizado para análise
- 🐳 **Docker** - Deploy fácil com docker-compose

## 🚀 Quick Start

```bash
# 1. Instalar dependências do sistema
./install-deps.sh

# 2. Buildar e subir
docker compose up --build -d

# 3. Resetar banco (primeira vez)
./reset-clickhouse.sh

# 4. Acessar
http://localhost:8080
Login: admin / admin123
```

## 📖 Documentação

- **[SETUP.md](SETUP.md)** - Setup completo do ambiente
- **[WORKFLOW.md](WORKFLOW.md)** - Workflow de desenvolvimento
- **[TESTING.md](TESTING.md)** - Como rodar testes

## 🏗️ Arquitetura

```
┌─────────────┐
│  Routers    │ NetFlow/IPFIX (UDP 2055)
└──────┬──────┘
       │
       ▼
┌──────────────────────────────┐
│   Rust Collector (Multi-threaded)   │
│  - NetFlow v9/IPFIX Parser   │
│  - Template Cache            │
│  - Flow Aggregation          │
│  - WebSocket Broadcaster     │
└──────┬───────────────┬───────┘
       │               │
       ▼               ▼
┌─────────────┐  ┌──────────┐
│ ClickHouse  │  │ Frontend │
│  Database   │  │ Vue 3 +  │
│             │  │ Chart.js │
└─────────────┘  └──────────┘
```

## 🎯 Componentes

### Backend (Rust)
- **collector-core**: NetFlow collector principal
- **netflow-parser**: Parser NetFlow v9/IPFIX
- **aggregator**: Agregação de flows em janelas de tempo
- **clickhouse-exporter**: Cliente ClickHouse
- **auth**: Autenticação JWT + SQLite

### Frontend (Vue 3)
- **Dashboard**: Visualização em tempo real
- **Auth**: Sistema de login
- **Pinia stores**: Gerenciamento de estado
- **Chart.js**: Gráficos interativos

## 📊 Stack Tecnológica

**Backend:**
- Rust (alta performance)
- Tokio (async runtime)
- Axum (web framework)
- ClickHouse (database)
- SQLite (auth)
- JWT (autenticação)

**Frontend:**
- Vue 3 (Composition API)
- TypeScript
- Pinia (state management)
- Tailwind CSS
- Chart.js
- WebSockets

## 🧪 Testing

```bash
# Validação rápida (30s)
./quick-check.sh

# Testes completos
make test

# Build completo com validação
./validate-and-build.sh
```

## 🛠️ Comandos Úteis

```bash
make help              # Ver todos os comandos
make test              # Rodar testes
make docker-build      # Buildar imagens
make reset-db          # Resetar ClickHouse
docker compose logs -f # Ver logs em tempo real
```

## 🔧 Configuração

### Gerar Flows (softflowd)

```bash
sudo softflowd -i wlp3s0 -n 127.0.0.1:2055 -v 9 -t maxlife=5
```

### Portas

- `2055/udp`: NetFlow/IPFIX input
- `8080`: Dashboard HTTP
- `8123`: ClickHouse HTTP
- `9000`: ClickHouse Native

## 📈 Performance

- **Throughput**: >1M flows/segundo
- **Latência**: <1ms agregação
- **Workers**: 4 threads paralelizadas
- **Buffer**: 32MB UDP recv buffer
- **Janela**: 1 segundo de agregação

## 🔐 Segurança

- Autenticação JWT (tokens de 24h)
- Senhas com bcrypt
- SQLite para usuários
- Admin padrão: `admin` / `admin123` (mude na produção!)

## 🤝 Contributing

1. Fork o projeto
2. Crie uma feature branch
3. Rode os testes: `make test`
4. Commit suas mudanças
5. Push para o branch
6. Abra um Pull Request

## 📝 License

MIT License - veja [LICENSE](LICENSE) para detalhes.

## 🆘 Suporte

- **Issues**: [GitHub Issues](https://github.com/seu-usuario/flow-collector/issues)
- **Docs**: Ver pasta `docs/` e arquivos `*.md`
- **Problemas comuns**: Ver [SETUP.md](SETUP.md#-problemas-comuns)
