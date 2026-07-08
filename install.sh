#!/usr/bin/env bash
# ============================================================
# FlowVision — instalador one-liner
#
# Uso rápido (sem clonar o repo):
#   curl -fsSL https://raw.githubusercontent.com/marcioelias/flowvision/main/install.sh | sudo bash
#
# Com versão específica:
#   FV_VERSION=1.1.0-beta.1 curl -fsSL ... | sudo bash
#
# Avançado:
#   sudo ./install.sh [--dir /opt/flowvision] [--version 1.1.0-beta.1] [--with-llm]
# ============================================================
set -euo pipefail

# ---- Parâmetros ---------------------------------------------
INSTALL_DIR="${INSTALL_DIR:-/opt/flowvision}"
FV_VERSION="${FV_VERSION:-latest}"
APP_USER="${APP_USER:-flowvision}"
COMPOSE_PROJECT="flow"
WITH_LLM=false

GHCR_OWNER="marcioelias"
RAW_BASE="https://raw.githubusercontent.com/${GHCR_OWNER}/flowvision/main"

# ---- Cores --------------------------------------------------
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

info()   { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()     { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()   { echo -e "${YELLOW}[WARN]${NC}  $*"; }
die()    { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }
header() { echo -e "\n${BOLD}${CYAN}==> $*${NC}"; }

[[ $EUID -eq 0 ]] || die "Execute como root: sudo $0"

# ---- Parse args ---------------------------------------------
while [[ $# -gt 0 ]]; do
  case $1 in
    --dir)       INSTALL_DIR="$2"; shift 2 ;;
    --version)   FV_VERSION="$2";  shift 2 ;;
    --with-llm)  WITH_LLM=true;    shift ;;
    *) warn "Argumento desconhecido: $1"; shift ;;
  esac
done

# ---- Banner -------------------------------------------------
echo -e "${BOLD}${CYAN}"
cat <<'BANNER'
  ███████╗██╗      ██████╗ ██╗    ██╗    ██╗   ██╗██╗███████╗██╗ ██████╗ ███╗   ██╗
  ██╔════╝██║     ██╔═══██╗██║    ██║    ██║   ██║██║██╔════╝██║██╔═══██╗████╗  ██║
  █████╗  ██║     ██║   ██║██║ █╗ ██║    ██║   ██║██║███████╗██║██║   ██║██╔██╗ ██║
  ██╔══╝  ██║     ██║   ██║██║███╗██║    ╚██╗ ██╔╝██║╚════██║██║██║   ██║██║╚██╗██║
  ██║     ███████╗╚██████╔╝╚███╔███╔╝     ╚████╔╝ ██║███████║██║╚██████╔╝██║ ╚████║
  ╚═╝     ╚══════╝ ╚═════╝  ╚══╝╚══╝       ╚═══╝  ╚═╝╚══════╝╚═╝ ╚═════╝ ╚═╝  ╚═══╝
BANNER
echo -e "${NC}"
info "Versão: ${FV_VERSION}  |  Destino: ${INSTALL_DIR}"

# ---- Detectar OS -------------------------------------------
# Usa subshell para evitar que o source do os-release sobrescreva variáveis do script
detect_os() {
  [[ -f /etc/os-release ]] || die "Não foi possível detectar o sistema operacional."
  OS_ID=$(   . /etc/os-release && echo "${ID:-unknown}")
  OS_FAMILY=$(. /etc/os-release && echo "${ID_LIKE:-$OS_ID}")
  OS_CODENAME=$(. /etc/os-release && echo "${VERSION_CODENAME:-}")
}

# ---- Instalar dependências ----------------------------------
install_deps() {
  header "Verificando dependências"

  if command -v docker &>/dev/null; then
    ok "Docker: $(docker --version | cut -d' ' -f3 | tr -d ',')"
  else
    header "Instalando Docker"
    case "$OS_FAMILY" in
      *debian*|*ubuntu*)
        apt-get update -qq
        apt-get install -y -qq ca-certificates curl gnupg
        install -m 0755 -d /etc/apt/keyrings
        curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
          | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
        chmod a+r /etc/apt/keyrings/docker.gpg
        echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
          https://download.docker.com/linux/${OS_ID} ${OS_CODENAME} stable" \
          > /etc/apt/sources.list.d/docker.list
        apt-get update -qq
        apt-get install -y -qq docker-ce docker-ce-cli containerd.io docker-compose-plugin
        ;;
      *rhel*|*fedora*|*centos*)
        dnf install -y docker docker-compose-plugin
        ;;
      *arch*)
        pacman -Sy --noconfirm docker docker-compose
        ;;
      *)
        curl -fsSL https://get.docker.com | sh
        ;;
    esac
    systemctl enable --now docker
    ok "Docker instalado."
  fi

  command -v curl &>/dev/null || { apt-get install -y -qq curl 2>/dev/null || dnf install -y curl; }
}

# ---- Criar usuário dedicado ---------------------------------
create_user() {
  header "Usuário de serviço"
  if id "$APP_USER" &>/dev/null; then
    ok "Usuário '$APP_USER' já existe."
  else
    useradd -r -s /sbin/nologin -d "$INSTALL_DIR" -M "$APP_USER"
    usermod -aG docker "$APP_USER"
    ok "Usuário '$APP_USER' criado."
  fi
}

# ---- Baixar arquivos de configuração -----------------------
download_files() {
  header "Baixando arquivos de configuração (v${FV_VERSION})"
  mkdir -p "$INSTALL_DIR"

  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

  if [[ -f "$SCRIPT_DIR/docker-compose.yml" ]] && [[ "$SCRIPT_DIR" != "$INSTALL_DIR" ]]; then
    info "Copiando docker-compose.yml do repo local..."
    cp "$SCRIPT_DIR/docker-compose.yml" "$INSTALL_DIR/docker-compose.yml"
  elif [[ ! -f "$INSTALL_DIR/docker-compose.yml" ]]; then
    info "Baixando docker-compose.yml do GitHub..."
    curl -fsSL "${RAW_BASE}/docker-compose.yml" -o "$INSTALL_DIR/docker-compose.yml"
  else
    info "docker-compose.yml já existe — mantendo."
  fi

  chown -R "$APP_USER:$APP_USER" "$INSTALL_DIR"
  ok "Arquivos prontos em $INSTALL_DIR"
}

# ---- Gerar .env ---------------------------------------------
setup_env() {
  header "Variáveis de ambiente"
  ENV_FILE="$INSTALL_DIR/.env"

  if [[ -f "$ENV_FILE" ]]; then
    if grep -q "^IMAGE_TAG=" "$ENV_FILE"; then
      sed -i "s|^IMAGE_TAG=.*|IMAGE_TAG=${FV_VERSION}|" "$ENV_FILE"
    else
      echo "IMAGE_TAG=${FV_VERSION}" >> "$ENV_FILE"
    fi
    if grep -q "insecure-default\|replace-with" "$ENV_FILE"; then
      NEW_SECRET=$(openssl rand -base64 48 | tr -d '\n/+=' | head -c 64)
      sed -i "s|JWT_SECRET=.*|JWT_SECRET=${NEW_SECRET}|" "$ENV_FILE"
      warn "JWT_SECRET era o valor padrão — foi regenerado."
    fi
    ok ".env existente mantido (IMAGE_TAG=${FV_VERSION})."
    return
  fi

  JWT_SECRET=$(openssl rand -base64 48 | tr -d '\n/+=' | head -c 64)

  cat > "$ENV_FILE" <<EOF
# Gerado por install.sh em $(date -u +"%Y-%m-%dT%H:%M:%SZ")
IMAGE_TAG=${FV_VERSION}
JWT_SECRET=${JWT_SECRET}
FLOW_RETENTION_DAYS=30
EOF

  chmod 600 "$ENV_FILE"
  chown "$APP_USER:$APP_USER" "$ENV_FILE"
  ok ".env criado."
}

# ---- Otimizações do kernel ----------------------------------
tune_kernel() {
  header "Parâmetros do kernel"
  cat > /etc/sysctl.d/90-flowvision.conf <<'EOF'
net.core.rmem_max = 134217728
net.core.rmem_default = 33554432
net.core.wmem_max = 134217728
net.ipv4.udp_mem = 102400 873800 16777216
net.core.netdev_max_backlog = 50000
EOF
  sysctl -p /etc/sysctl.d/90-flowvision.conf &>/dev/null
  ok "Buffers UDP aumentados."
}

# ---- Serviço systemd ----------------------------------------
install_service() {
  header "Serviço systemd"
  cat > /etc/systemd/system/flowvision.service <<EOF
[Unit]
Description=FlowVision — NetFlow/IPFIX Collector
After=network-online.target docker.service
Wants=network-online.target
Requires=docker.service

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=${INSTALL_DIR}
EnvironmentFile=${INSTALL_DIR}/.env
ExecStartPre=-/usr/bin/docker compose -p ${COMPOSE_PROJECT} pull --quiet
ExecStart=/usr/bin/docker compose -p ${COMPOSE_PROJECT} up -d --remove-orphans
ExecStop=/usr/bin/docker compose -p ${COMPOSE_PROJECT} down
User=${APP_USER}
StandardOutput=journal
StandardError=journal
TimeoutStartSec=300
TimeoutStopSec=120
Restart=on-failure
RestartSec=15s

[Install]
WantedBy=multi-user.target
EOF

  systemctl daemon-reload
  systemctl enable flowvision
  ok "Serviço 'flowvision' habilitado no boot."
}

# ---- Firewall -----------------------------------------------
configure_firewall() {
  header "Firewall"
  if command -v ufw &>/dev/null && ufw status 2>/dev/null | grep -q "active"; then
    ufw allow 2055/udp comment "FlowVision NetFlow/IPFIX" &>/dev/null
    ufw allow 8080/tcp comment "FlowVision dashboard"     &>/dev/null
    ok "UFW: portas 2055/udp e 8080/tcp abertas."
  elif command -v firewall-cmd &>/dev/null && systemctl is-active --quiet firewalld; then
    firewall-cmd --permanent --add-port=2055/udp &>/dev/null
    firewall-cmd --permanent --add-port=8080/tcp &>/dev/null
    firewall-cmd --reload &>/dev/null
    ok "firewalld: portas 2055/udp e 8080/tcp abertas."
  else
    warn "Nenhum firewall ativo. Abra manualmente: 2055/udp e 8080/tcp."
  fi
}

# ---- Instalar utilitários -----------------------------------
install_tools() {
  header "Utilitários"

  cat > /usr/local/bin/flowvision-update <<SCRIPT
#!/usr/bin/env bash
set -euo pipefail
INSTALL_DIR="${INSTALL_DIR}"
VER="\${1:-latest}"
echo "Atualizando FlowVision para versão: \${VER}..."
sed -i "s|^IMAGE_TAG=.*|IMAGE_TAG=\${VER}|" "\${INSTALL_DIR}/.env"
cd "\${INSTALL_DIR}"
docker compose pull --quiet
systemctl restart flowvision
docker image prune -f &>/dev/null || true
echo "Atualizado para \${VER}."
systemctl status flowvision --no-pager
SCRIPT
  chmod +x /usr/local/bin/flowvision-update

  cat > /usr/local/bin/flowvision-status <<SCRIPT
#!/usr/bin/env bash
echo "=== Serviço ==="
systemctl status flowvision --no-pager -l
echo ""
echo "=== Containers ==="
docker compose -p flow -f ${INSTALL_DIR}/docker-compose.yml ps
echo ""
echo "=== Recursos ==="
docker stats --no-stream --format "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}" 2>/dev/null || true
SCRIPT
  chmod +x /usr/local/bin/flowvision-status

  ok "Comandos: flowvision-update [versão], flowvision-status"
}

# ---- Iniciar ------------------------------------------------
start_services() {
  header "Iniciando serviços"
  cd "$INSTALL_DIR"

  COMPOSE_ARGS=()
  [[ "$WITH_LLM" == true ]] && COMPOSE_ARGS+=("--profile" "llm")

  sudo -u "$APP_USER" docker compose "${COMPOSE_ARGS[@]}" pull --quiet
  sudo -u "$APP_USER" docker compose "${COMPOSE_ARGS[@]}" up -d --remove-orphans

  info "Aguardando ClickHouse ficar saudável..."
  for i in $(seq 1 24); do
    STATUS=$(docker inspect ch-database --format='{{.State.Health.Status}}' 2>/dev/null || echo "waiting")
    [[ "$STATUS" == "healthy" ]] && break
    [[ $i -eq 24 ]] && { warn "ClickHouse ainda inicializando. Verifique: docker logs ch-database"; break; }
    sleep 5 && echo -n "."
  done
  echo ""
  ok "Serviços iniciados."
}

# ---- Sumário ------------------------------------------------
print_summary() {
  IP=$(hostname -I | awk '{print $1}')
  echo ""
  echo -e "${BOLD}${GREEN}╔══════════════════════════════════════════════════╗"
  echo -e "║     FlowVision instalado com sucesso!           ║"
  echo -e "╚══════════════════════════════════════════════════╝${NC}"
  echo ""
  echo -e "  ${BOLD}Dashboard:${NC}     http://${IP}:8080"
  echo -e "  ${BOLD}Login padrão:${NC}  admin / admin123"
  echo -e "  ${BOLD}NetFlow/IPFIX:${NC} UDP ${IP}:2055"
  echo ""
  echo -e "  ${BOLD}Versão instalada:${NC} ${FV_VERSION}"
  echo -e "  ${BOLD}Atualizar:${NC}     flowvision-update [versão]"
  echo -e "  ${BOLD}Status:${NC}        flowvision-status"
  echo -e "  ${BOLD}Logs:${NC}          journalctl -u flowvision -f"
  [[ "$WITH_LLM" == true ]] && echo -e "  ${BOLD}Ollama (LLM):${NC}    http://${IP}:11434"
  echo ""
  warn "Troque a senha padrão após o primeiro acesso!"
  echo ""
}

# ---- Main ---------------------------------------------------
detect_os
install_deps
create_user
download_files
setup_env
tune_kernel
install_service
install_tools
configure_firewall
start_services
print_summary
