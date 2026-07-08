# Task 8.2 — ExaBGP no Docker + Geração de Config

**Status:** 🔲 A implementar  
**Fase:** 8 — BGP Announcement via ExaBGP  
**Depende de:** task 8.1 (tabelas BGP no SQLite)

---

## Objetivo

1. Adicionar o container ExaBGP ao `docker-compose.yml`
2. Implementar a geração do `exabgp.conf` a partir dos dados do SQLite
3. Implementar o mecanismo de aplicação de config (escrita de arquivo + SIGUSR1)

---

## Por que `network_mode: host`

BGP (TCP porta 179) requer que o IP de origem da conexão seja o mesmo que o peer
tem configurado como vizinho. Com bridge Docker + NAT, o IP de origem seria o do host
(mascarado) — o que pode funcionar em alguns casos mas quebra **MD5 TCP authentication**
(que usa o IP exato como chave) e impede que o peer inicie a conexão para o container.

**Solução:** `network_mode: host` — o container usa diretamente a stack de rede do host.

> **Implicação:** o ExaBGP não estará na `flow-net` bridge. A comunicação com o
> `flow-collector` é feita via **named pipe em volume compartilhado**, não via rede.

---

## Mecanismo de controle: named pipe

ExaBGP lê comandos de um named pipe (FIFO) que o processo controlador alimenta.
O `flow-collector` escreve nesse pipe para anunciar/retirar rotas.

```
/run/exabgp/
├── exabgp.in   ← FIFO: collector escreve, exabgp lê
└── exabgp.out  ← FIFO: exabgp escreve eventos, collector lê (opcional)
```

Ambos os containers montam `/run/exabgp` como volume compartilhado.

---

## Alterações no `docker-compose.yml`

```yaml
services:
  # ... clickhouse, flow-collector, dashboard (sem alteração na rede deles)

  exabgp:
    image: pierky/exabgp:4.2.11      # versão fixa
    container_name: exabgp
    network_mode: host               # obrigatório para BGP real
    volumes:
      - exabgp_config:/etc/exabgp    # exabgp.conf gerado pelo collector
      - exabgp_run:/run/exabgp       # named pipes (FIFO) de controle
    restart: unless-stopped
    depends_on:
      flow-collector:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "pgrep", "-x", "exabgp"]
      interval: 15s
      timeout: 5s
      retries: 3
      start_period: 10s

volumes:
  clickhouse_data:
  auth_data:
  exabgp_config:   # novo
  exabgp_run:      # novo
```

O `flow-collector` também precisa montar os dois volumes:
```yaml
  flow-collector:
    volumes:
      - auth_data:/app
      - exabgp_config:/run/exabgp-config  # lê/escreve exabgp.conf
      - exabgp_run:/run/exabgp            # escreve no FIFO
```

---

## Formato do `exabgp.conf` gerado

O arquivo é gerado 100% a partir do banco — nunca editado manualmente.

```
# GERADO AUTOMATICAMENTE — NÃO EDITE
# FlowVision BGP Controller — gerado em {timestamp}

process flowvision-controller {
    run /bin/sh -c 'while true; do cat /run/exabgp/exabgp.in; done';
    encoder text;
}

neighbor {neighbor_ip} {
    description "{name}";
    router-id {local_ip};
    local-address {local_ip};
    local-as {local_as};
    peer-as {peer_as};
    hold-time {hold_time};
    {md5_line}   # omitido se NULL

    family {
        ipv4 unicast;
    }

    api {
        processes [ flowvision-controller ];
    }
}

# bloco repetido para cada peer enabled
```

> **`router-id`**: usa o `local_ip` do primeiro peer. Se houver múltiplos peers com
> IPs locais diferentes, usa o primeiro alfabeticamente. Pode ser promovido a
> setting em task futura.

---

## Módulo Rust: `collector-core/src/bgp_config.rs`

```rust
// Gera o conteúdo do exabgp.conf a partir dos peers ativos no banco
pub async fn generate_config(pool: &SqlitePool) -> anyhow::Result<String>

// Escreve o arquivo e sinaliza reload
pub async fn apply_config(pool: &SqlitePool, config_path: &str) -> anyhow::Result<()>
    // 1. generate_config(pool)
    // 2. tokio::fs::write(config_path, content)
    // 3. Encontra o PID do exabgp via /var/run/exabgp/exabgp.pid ou `pgrep exabgp`
    //    (em host network, o PID do container é visível via /proc do host)
    //    Alternativa mais robusta: signal via docker exec (ver nota abaixo)
    // 4. kill(pid, SIGUSR1)
```

> **Nota sobre SIGUSR1 em Docker:** como o ExaBGP está em `network_mode: host` mas
> é um processo separado, a forma mais confiável de enviar SIGUSR1 é:
> - Expor o socket do Docker (`/var/run/docker.sock`) no container do collector
> - Usar a Docker API para executar `kill -USR1 1` dentro do container exabgp
>
> Alternativa mais simples: escrever o comando `reload` no named pipe FIFO
> (suportado pelo ExaBGP 4.x como comando de controle).
>
> **Implementar com FIFO primeiro** — se não funcionar, escalar para Docker socket.

---

## Inicialização do named pipe

O collector precisa criar os FIFOs se não existirem (ExaBGP os cria, mas pode haver
race condition):

```rust
// Na startup, após init_bgp_tables:
pub fn ensure_fifos(run_dir: &str) -> std::io::Result<()> {
    // mkfifo /run/exabgp/exabgp.in se não existir (via nix::unistd::mkfifo)
    // mkfifo /run/exabgp/exabgp.out se não existir
}
```

---

## Re-anúncio na startup

Ao iniciar, o collector lê todos os `bgp_announcements` com `withdrawn_at IS NULL`
e os re-anuncia no FIFO. Isso garante que um restart do collector não perde o estado
de anúncios ativos.

---

## Paths configuráveis via env vars

| Var | Padrão | Uso |
|-----|--------|-----|
| `EXABGP_CONFIG_PATH` | `/run/exabgp-config/exabgp.conf` | Onde escrever o config |
| `EXABGP_PIPE_PATH` | `/run/exabgp/exabgp.in` | FIFO de controle |

---

## Aceite

- [ ] Container ExaBGP aparece em `docker-compose up` sem erros
- [ ] `generate_config` produz conf válido para 1 e N peers
- [ ] Peers com `enabled = false` são omitidos do conf
- [ ] Peer com `md5_password = NULL` não inclui a linha `md5-password`
- [ ] `apply_config` escreve o arquivo e envia sinal de reload
- [ ] Re-anúncio na startup restaura rotas ativas após restart do collector
