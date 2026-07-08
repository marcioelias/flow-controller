#!/usr/bin/env bash
# Atualiza o FlowVision para uma nova versão de imagem.
# Uso: sudo ./update.sh [versão]
# Exemplo: sudo ./update.sh 1.2.0
# Sem argumento: atualiza para :latest
set -euo pipefail

INSTALL_DIR="${INSTALL_DIR:-/opt/flowvision}"
VERSION="${1:-latest}"
COMPOSE_PROJECT="flow"

[[ $EUID -eq 0 ]] || { echo "Execute como root: sudo $0 [versão]" >&2; exit 1; }

echo "Atualizando FlowVision para versão: ${VERSION}..."

ENV_FILE="${INSTALL_DIR}/.env"
if grep -q "^VERSION=" "$ENV_FILE" 2>/dev/null; then
  sed -i "s|^VERSION=.*|VERSION=${VERSION}|" "$ENV_FILE"
else
  echo "VERSION=${VERSION}" >> "$ENV_FILE"
fi

cd "$INSTALL_DIR"
docker compose -p "$COMPOSE_PROJECT" pull --quiet
docker compose -p "$COMPOSE_PROJECT" up -d --remove-orphans
docker image prune -f &>/dev/null || true

echo "Atualizado para ${VERSION}."
docker compose -p "$COMPOSE_PROJECT" ps
