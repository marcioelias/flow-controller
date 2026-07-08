.PHONY: test test-backend test-frontend build build-backend build-frontend docker-build docker-up docker-down clean check-warnings lint help

# Colors
YELLOW := \033[1;33m
GREEN := \033[0;32m
RED := \033[0;31m
NC := \033[0m

help:
	@echo "$(YELLOW)Comandos disponíveis:$(NC)"
	@echo "  $(GREEN)make test$(NC)           - Roda todos os testes (backend + frontend)"
	@echo "  $(GREEN)make test-backend$(NC)   - Roda apenas testes do backend"
	@echo "  $(GREEN)make test-frontend$(NC)  - Roda apenas testes do frontend"
	@echo "  $(GREEN)make lint$(NC)           - Verifica código (clippy + warnings)"
	@echo "  $(GREEN)make check-warnings$(NC) - Verifica se há warnings"
	@echo "  $(GREEN)make build$(NC)          - Builda tudo localmente"
	@echo "  $(GREEN)make docker-build$(NC)   - Builda as imagens Docker"
	@echo "  $(GREEN)make docker-up$(NC)      - Sobe os containers"
	@echo "  $(GREEN)make docker-down$(NC)    - Para os containers"
	@echo "  $(GREEN)make clean$(NC)          - Limpa builds"
	@echo "  $(GREEN)make reset-db$(NC)       - Reseta o ClickHouse"
	@echo "  $(GREEN)make install$(NC)        - Instala no servidor baremetal (requer root)"
	@echo "  $(GREEN)make update$(NC)         - Atualiza a instalação no servidor"
	@echo "  $(GREEN)make rollback$(NC)       - Rollback para versão anterior"

test: test-backend test-frontend
	@echo "$(GREEN)✓ Todos os testes passaram!$(NC)"

test-backend:
	@echo "$(YELLOW)Rodando testes backend...$(NC)"
	@cargo test

test-frontend:
	@echo "$(YELLOW)Rodando testes frontend...$(NC)"
	@cd frontend && npm run test:run

check-warnings:
	@echo "$(YELLOW)Verificando warnings...$(NC)"
	@./check-warnings.sh

lint:
	@echo "$(YELLOW)Rodando linters...$(NC)"
	@echo "$(YELLOW)  Backend (clippy)...$(NC)"
	@cargo clippy -- -D warnings
	@echo "$(YELLOW)  Frontend (eslint)...$(NC)"
	@cd frontend && npm run lint || true

build: build-backend build-frontend
	@echo "$(GREEN)✓ Build completo!$(NC)"

build-backend:
	@echo "$(YELLOW)Buildando backend...$(NC)"
	@cargo build --release

build-frontend:
	@echo "$(YELLOW)Buildando frontend...$(NC)"
	@cd frontend && npm install && npm run build

docker-build:
	@echo "$(YELLOW)Buildando imagens Docker...$(NC)"
	@docker compose build

docker-up:
	@echo "$(YELLOW)Subindo containers...$(NC)"
	@docker compose up -d

docker-down:
	@echo "$(YELLOW)Parando containers...$(NC)"
	@docker compose down

reset-db:
	@echo "$(YELLOW)Resetando ClickHouse...$(NC)"
	@./reset-clickhouse.sh

clean:
	@echo "$(YELLOW)Limpando builds...$(NC)"
	@cargo clean
	@cd frontend && rm -rf dist node_modules

dev-backend:
	@echo "$(YELLOW)Rodando backend em modo dev...$(NC)"
	@cargo run

dev-frontend:
	@echo "$(YELLOW)Rodando frontend em modo dev...$(NC)"
	@cd frontend && npm run dev

install:
	@echo "$(YELLOW)Instalando no servidor baremetal...$(NC)"
	@sudo ./install.sh

update:
	@echo "$(YELLOW)Atualizando instalação...$(NC)"
	@sudo ./update.sh

rollback:
	@echo "$(YELLOW)Executando rollback...$(NC)"
	@sudo ./update.sh --rollback
