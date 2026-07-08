#!/usr/bin/env bash
# ============================================================
# flow-collector — script de atualização
# Uso: sudo ./update.sh [--rollback] [--no-rebuild]
# ============================================================
set -euo pipefail

INSTALL_DIR="${INSTALL_DIR:-/opt/flow-collector}"
SERVICE_NAME="flow-collector"
COMPOSE_PROJECT="flow"
BACKUP_DIR="${INSTALL_DIR}/.backups"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || error "Execute como root: sudo $0"

ROLLBACK=false
NO_REBUILD=false

while [[ $# -gt 0 ]]; do
  case $1 in
    --rollback)   ROLLBACK=true; shift ;;
    --no-rebuild) NO_REBUILD=true; shift ;;
    *) warn "Argumento desconhecido: $1"; shift ;;
  esac
done

cd "$INSTALL_DIR"

# ---- Rollback -----------------------------------------------
if [[ "$ROLLBACK" == "true" ]]; then
  LATEST_BACKUP=$(ls -t "$BACKUP_DIR"/*.env 2>/dev/null | head -1 || true)
  [[ -z "$LATEST_BACKUP" ]] && error "Nenhum backup encontrado em $BACKUP_DIR"

  warn "Executando rollback para: $LATEST_BACKUP"
  cp "$LATEST_BACKUP" "$INSTALL_DIR/.env"

  LATEST_COMPOSE=$(ls -t "$BACKUP_DIR"/*.docker-compose.yml 2>/dev/null | head -1 || true)
  if [[ -n "$LATEST_COMPOSE" ]]; then
    cp "$LATEST_COMPOSE" "$INSTALL_DIR/docker-compose.yml"
    info "docker-compose.yml restaurado."
  fi

  systemctl restart "$SERVICE_NAME"
  ok "Rollback concluído. Serviço reiniciado."
  exit 0
fi

# ---- Backup antes de atualizar ------------------------------
mkdir -p "$BACKUP_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
cp "$INSTALL_DIR/.env" "$BACKUP_DIR/${TIMESTAMP}.env"
cp "$INSTALL_DIR/docker-compose.yml" "$BACKUP_DIR/${TIMESTAMP}.docker-compose.yml"
info "Backup salvo em $BACKUP_DIR/${TIMESTAMP}.*"

# Manter apenas os 5 backups mais recentes
ls -t "$BACKUP_DIR"/*.env 2>/dev/null | tail -n +6 | xargs rm -f || true

# ---- Pull do git se disponível ------------------------------
if [[ -d "$INSTALL_DIR/.git" ]]; then
  info "Atualizando código-fonte..."
  git -C "$INSTALL_DIR" pull --ff-only || warn "git pull falhou — continuando com versão atual."
fi

# ---- Rebuild das imagens ------------------------------------
if [[ "$NO_REBUILD" == "false" ]]; then
  info "Baixando novas imagens base..."
  docker compose -p "$COMPOSE_PROJECT" pull --quiet || true

  info "Rebuilding imagens da aplicação..."
  docker compose -p "$COMPOSE_PROJECT" build --no-cache
fi

# ---- Restart com zero-downtime (rolling) --------------------
info "Atualizando containers com downtime mínimo..."

# Subir nova versão e deixar o Compose gerenciar a substituição
docker compose -p "$COMPOSE_PROJECT" up -d --remove-orphans

# ---- Healthcheck pós-update ---------------------------------
info "Verificando saúde dos containers..."
sleep 5

UNHEALTHY=()
for container in ch-database rust-collector vue-dashboard; do
  STATUS=$(docker inspect "$container" --format='{{.State.Health.Status}}' 2>/dev/null || echo "no-healthcheck")
  if [[ "$STATUS" == "unhealthy" ]]; then
    UNHEALTHY+=("$container")
  fi
done

if [[ ${#UNHEALTHY[@]} -gt 0 ]]; then
  warn "Containers com problema: ${UNHEALTHY[*]}"
  warn "Para fazer rollback: sudo $0 --rollback"
else
  ok "Todos os containers saudáveis."
fi

# ---- Limpeza de imagens antigas -----------------------------
info "Limpando imagens obsoletas..."
docker image prune -f &>/dev/null || true

echo ""
ok "Atualização concluída!"
docker compose -p "$COMPOSE_PROJECT" ps
