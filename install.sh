#!/usr/bin/env bash
# ============================================================
# flow-collector — instalador baremetal
# Suporta: Ubuntu/Debian, RHEL/Fedora/CentOS, Arch/Manjaro
# Uso: curl -fsSL <url>/install.sh | sudo bash
#   OU: sudo ./install.sh [--dir /opt/flow-collector]
# ============================================================
set -euo pipefail

# ---- Variáveis configuráveis --------------------------------
INSTALL_DIR="${INSTALL_DIR:-/opt/flow-collector}"
SERVICE_NAME="flow-collector"
COMPOSE_PROJECT="flow"
REPO_URL="${REPO_URL:-}"      # preenchido via env se clonar do git
APP_USER="${APP_USER:-flowcollector}"

# ---- Cores --------------------------------------------------
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

info()    { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()      { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }
header()  { echo -e "\n${BOLD}${CYAN}==> $*${NC}"; }

# ---- Checar root --------------------------------------------
[[ $EUID -eq 0 ]] || error "Execute como root: sudo $0"

# ---- Parse args ---------------------------------------------
while [[ $# -gt 0 ]]; do
  case $1 in
    --dir) INSTALL_DIR="$2"; shift 2 ;;
    --repo) REPO_URL="$2"; shift 2 ;;
    *) warn "Argumento desconhecido: $1"; shift ;;
  esac
done

# ---- Detectar OS -------------------------------------------
detect_os() {
  if [[ -f /etc/os-release ]]; then
    . /etc/os-release
    OS_ID="${ID:-unknown}"
    OS_FAMILY="${ID_LIKE:-$OS_ID}"
  else
    error "Não foi possível detectar o sistema operacional."
  fi
}

# ---- Instalar Docker ----------------------------------------
install_docker() {
  if command -v docker &>/dev/null; then
    ok "Docker já instalado: $(docker --version)"
    return
  fi

  header "Instalando Docker"
  case "$OS_FAMILY" in
    *debian*|*ubuntu*)
      apt-get update -qq
      apt-get install -y -qq ca-certificates curl gnupg lsb-release
      install -m 0755 -d /etc/apt/keyrings
      curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
        | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
      chmod a+r /etc/apt/keyrings/docker.gpg
      echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
        https://download.docker.com/linux/${OS_ID} $(lsb_release -cs) stable" \
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
      info "Tentando instalação genérica via get.docker.com..."
      curl -fsSL https://get.docker.com | sh
      ;;
  esac

  systemctl enable --now docker
  ok "Docker instalado."
}

# ---- Criar usuário dedicado ---------------------------------
create_app_user() {
  if id "$APP_USER" &>/dev/null; then
    ok "Usuário '$APP_USER' já existe."
  else
    header "Criando usuário '$APP_USER'"
    useradd -r -s /sbin/nologin -d "$INSTALL_DIR" -M "$APP_USER"
    usermod -aG docker "$APP_USER"
    ok "Usuário criado e adicionado ao grupo docker."
  fi
}

# ---- Copiar ou clonar os arquivos ---------------------------
setup_files() {
  header "Configurando arquivos em $INSTALL_DIR"
  mkdir -p "$INSTALL_DIR"

  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

  if [[ -f "$SCRIPT_DIR/docker-compose.yml" ]]; then
    # Rodando dentro do próprio repo — copiar arquivos
    info "Copiando arquivos do repositório local..."
    rsync -a --exclude='.git' --exclude='target' --exclude='node_modules' \
      "$SCRIPT_DIR/" "$INSTALL_DIR/"
  elif [[ -n "$REPO_URL" ]]; then
    # Clonar de um repositório remoto
    if ! command -v git &>/dev/null; then
      case "$OS_FAMILY" in
        *debian*|*ubuntu*) apt-get install -y -qq git ;;
        *rhel*|*fedora*|*centos*) dnf install -y git ;;
        *arch*) pacman -Sy --noconfirm git ;;
      esac
    fi
    if [[ -d "$INSTALL_DIR/.git" ]]; then
      info "Repositório já existe — atualizando..."
      git -C "$INSTALL_DIR" pull
    else
      git clone "$REPO_URL" "$INSTALL_DIR"
    fi
  else
    error "Nenhum repositório encontrado. Execute dentro do repo ou use --repo <url>."
  fi

  chown -R "$APP_USER:$APP_USER" "$INSTALL_DIR"
  ok "Arquivos prontos."
}

# ---- Gerar .env com segredos --------------------------------
setup_env() {
  header "Configurando variáveis de ambiente"
  ENV_FILE="$INSTALL_DIR/.env"

  if [[ -f "$ENV_FILE" ]]; then
    warn ".env já existe — mantendo configuração atual."
    # Garante que JWT_SECRET não é o valor de exemplo
    if grep -q "replace-with-a-64-char" "$ENV_FILE"; then
      warn "JWT_SECRET ainda é o valor padrão. Gerando novo segredo..."
      NEW_SECRET=$(openssl rand -base64 48 | tr -d '\n/+=' | head -c 64)
      sed -i "s|JWT_SECRET=.*|JWT_SECRET=${NEW_SECRET}|" "$ENV_FILE"
      ok "JWT_SECRET atualizado."
    fi
    return
  fi

  JWT_SECRET=$(openssl rand -base64 48 | tr -d '\n/+=' | head -c 64)

  cat > "$ENV_FILE" <<EOF
# Gerado automaticamente por install.sh em $(date -u +"%Y-%m-%dT%H:%M:%SZ")
# ATENÇÃO: mantenha este arquivo seguro — contém segredos de produção

JWT_SECRET=${JWT_SECRET}
FLOW_RETENTION_DAYS=30
EOF

  chmod 600 "$ENV_FILE"
  chown "$APP_USER:$APP_USER" "$ENV_FILE"
  ok ".env criado com JWT_SECRET gerado aleatoriamente."
}

# ---- Criar serviço systemd ----------------------------------
install_systemd_service() {
  header "Instalando serviço systemd"
  SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

  cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=Flow Collector (NetFlow/IPFIX)
Documentation=https://github.com/$(basename "$INSTALL_DIR")
After=network-online.target docker.service
Wants=network-online.target
Requires=docker.service

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=${INSTALL_DIR}
EnvironmentFile=${INSTALL_DIR}/.env
ExecStartPre=/usr/bin/docker compose -p ${COMPOSE_PROJECT} pull --quiet || true
ExecStart=/usr/bin/docker compose -p ${COMPOSE_PROJECT} up -d --build --remove-orphans
ExecStop=/usr/bin/docker compose -p ${COMPOSE_PROJECT} down
ExecReload=/usr/bin/docker compose -p ${COMPOSE_PROJECT} restart
User=${APP_USER}
StandardOutput=journal
StandardError=journal
TimeoutStartSec=300
TimeoutStopSec=120
Restart=on-failure
RestartSec=10s

[Install]
WantedBy=multi-user.target
EOF

  systemctl daemon-reload
  systemctl enable "$SERVICE_NAME"
  ok "Serviço '${SERVICE_NAME}' registrado e habilitado no boot."
}

# ---- Configurar firewall (UFW/firewalld) --------------------
configure_firewall() {
  header "Configurando firewall"

  if command -v ufw &>/dev/null && ufw status | grep -q "Status: active"; then
    info "UFW detectado. Abrindo portas necessárias..."
    ufw allow 2055/udp comment "NetFlow/IPFIX collector"
    ufw allow 8080/tcp comment "Flow Collector dashboard"
    ok "Regras UFW adicionadas."
  elif command -v firewall-cmd &>/dev/null && systemctl is-active --quiet firewalld; then
    info "firewalld detectado. Abrindo portas necessárias..."
    firewall-cmd --permanent --add-port=2055/udp
    firewall-cmd --permanent --add-port=8080/tcp
    firewall-cmd --reload
    ok "Regras firewalld adicionadas."
  else
    warn "Nenhum firewall ativo detectado. Abra manualmente as portas 2055/udp e 8080/tcp."
  fi
}

# ---- Instalar script de update e logrotate -----------------
install_extras() {
  header "Instalando utilitários extras"

  # Script de atualização
  cat > /usr/local/bin/flow-collector-update <<'UPDATESCRIPT'
#!/usr/bin/env bash
set -euo pipefail
INSTALL_DIR="/opt/flow-collector"
cd "$INSTALL_DIR"
echo "Parando serviço..."
systemctl stop flow-collector
echo "Baixando atualizações..."
git pull 2>/dev/null || true
echo "Rebuilding imagens..."
docker compose pull --quiet || true
docker compose build --no-cache
echo "Iniciando serviço..."
systemctl start flow-collector
echo "Limpando imagens antigas..."
docker image prune -f
echo "Atualização concluída!"
systemctl status flow-collector --no-pager
UPDATESCRIPT
  chmod +x /usr/local/bin/flow-collector-update

  # Logrotate para journal do serviço
  cat > /etc/logrotate.d/flow-collector <<'LOGROTATE'
/var/log/flow-collector.log {
    daily
    missingok
    rotate 14
    compress
    delaycompress
    notifempty
    sharedscripts
}
LOGROTATE

  # Script de status
  cat > /usr/local/bin/flow-collector-status <<STATUSSCRIPT
#!/usr/bin/env bash
echo ""
echo "=== Serviço systemd ==="
systemctl status flow-collector --no-pager -l
echo ""
echo "=== Containers ==="
docker compose -p flow -f ${INSTALL_DIR}/docker-compose.yml ps
echo ""
echo "=== Uso de recursos ==="
docker stats --no-stream --format "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}" \
  ch-database rust-collector vue-dashboard 2>/dev/null || true
STATUSSCRIPT
  chmod +x /usr/local/bin/flow-collector-status

  ok "Comandos disponíveis: flow-collector-update, flow-collector-status"
}

# ---- Ajuste de performance do kernel ------------------------
tune_kernel() {
  header "Otimizando parâmetros do kernel para alto volume UDP"
  SYSCTL_FILE="/etc/sysctl.d/90-flow-collector.conf"

  cat > "$SYSCTL_FILE" <<'EOF'
# Buffer UDP para alto volume de NetFlow
net.core.rmem_max = 134217728
net.core.rmem_default = 33554432
net.core.wmem_max = 134217728
net.core.wmem_default = 33554432
net.ipv4.udp_mem = 102400 873800 16777216
# Backlog de conexões
net.core.netdev_max_backlog = 50000
EOF

  sysctl -p "$SYSCTL_FILE" &>/dev/null
  ok "Parâmetros do kernel aplicados."
}

# ---- Iniciar os serviços ------------------------------------
start_services() {
  header "Iniciando flow-collector"
  cd "$INSTALL_DIR"

  # Build e start via Docker Compose direto (systemd cuida dos restarts)
  sudo -u "$APP_USER" docker compose -p "$COMPOSE_PROJECT" up -d --build 2>&1 | \
    grep -E "^(#|=>|ERROR|error)" || true

  systemctl start "$SERVICE_NAME" 2>/dev/null || true

  # Aguardar healthcheck do ClickHouse
  info "Aguardando ClickHouse ficar saudável..."
  TRIES=0
  until docker inspect ch-database --format='{{.State.Health.Status}}' 2>/dev/null \
    | grep -q "healthy" || [[ $TRIES -ge 24 ]]; do
    sleep 5
    TRIES=$((TRIES+1))
    echo -n "."
  done
  echo ""

  if docker inspect ch-database --format='{{.State.Health.Status}}' 2>/dev/null | grep -q "healthy"; then
    ok "ClickHouse saudável."
  else
    warn "ClickHouse pode ainda estar inicializando. Verifique com: docker logs ch-database"
  fi
}

# ---- Sumário ------------------------------------------------
print_summary() {
  SERVER_IP=$(hostname -I | awk '{print $1}')
  echo ""
  echo -e "${BOLD}${GREEN}╔══════════════════════════════════════════════════════╗"
  echo -e "║       Flow Collector instalado com sucesso!          ║"
  echo -e "╚══════════════════════════════════════════════════════╝${NC}"
  echo ""
  echo -e "  ${BOLD}Dashboard:${NC}    http://${SERVER_IP}:8080"
  echo -e "  ${BOLD}Login:${NC}        admin / admin123  (troque após o primeiro acesso)"
  echo -e "  ${BOLD}NetFlow/IPFIX:${NC} UDP ${SERVER_IP}:2055"
  echo -e "  ${BOLD}Métricas:${NC}     http://${SERVER_IP}:3000/metrics  (Prometheus)"
  echo ""
  echo -e "  ${BOLD}Comandos úteis:${NC}"
  echo "    flow-collector-status   — ver estado dos containers"
  echo "    flow-collector-update   — atualizar para nova versão"
  echo "    systemctl restart flow-collector"
  echo "    journalctl -u flow-collector -f"
  echo ""
  echo -e "  ${BOLD}Arquivos:${NC}"
  echo "    Config:  $INSTALL_DIR/.env"
  echo "    Compose: $INSTALL_DIR/docker-compose.yml"
  echo ""
  warn "Troque a senha padrão do dashboard imediatamente em produção!"
  echo ""
}

# ---- Main ---------------------------------------------------
main() {
  echo -e "${BOLD}${CYAN}"
  echo "  ███████╗██╗      ██████╗ ██╗    ██╗"
  echo "  ██╔════╝██║     ██╔═══██╗██║    ██║"
  echo "  █████╗  ██║     ██║   ██║██║ █╗ ██║"
  echo "  ██╔══╝  ██║     ██║   ██║██║███╗██║"
  echo "  ██║     ███████╗╚██████╔╝╚███╔███╔╝"
  echo "  ╚═╝     ╚══════╝ ╚═════╝  ╚══╝╚══╝"
  echo -e "  Collector  —  Instalador Baremetal${NC}"
  echo ""

  detect_os
  info "Sistema detectado: ${OS_ID} (família: ${OS_FAMILY})"

  install_docker
  create_app_user
  setup_files
  setup_env
  tune_kernel
  install_systemd_service
  install_extras
  configure_firewall
  start_services
  print_summary
}

main "$@"
