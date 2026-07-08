# Task 8.3 — Bridge de Controle (Announce / Withdraw)

**Status:** 🔲 A implementar  
**Fase:** 8 — BGP Announcement via ExaBGP  
**Depende de:** task 8.1, task 8.2

---

## Objetivo

Implementar as funções Rust que gravam comandos no named pipe do ExaBGP para
anunciar e retirar prefixos, e persistem o estado no SQLite.

---

## Mecanismo

O ExaBGP lê comandos texto do FIFO `/run/exabgp/exabgp.in`.
O collector abre o pipe em modo append (não bloqueia) e escreve uma linha por vez.

```
announce route 192.0.2.0/24 next-hop 192.0.2.254 community [65000:9999]
withdraw route 192.0.2.0/24
```

Para um peer específico, prefixar com `neighbor <ip>`:
```
neighbor 10.0.0.1 announce route 192.0.2.0/24 next-hop self community [65000:9999]
neighbor 10.0.0.1 withdraw route 192.0.2.0/24
```

> `next-hop self` usa o `local_ip` do peer configurado — ExaBGP resolve automaticamente.

---

## Módulo Rust: `collector-core/src/bgp_control.rs`

```rust
pub struct AnnounceRequest {
    pub prefix: String,              // "192.0.2.0/24"
    pub next_hop: String,            // "192.0.2.254" ou "self"
    pub community_id: Option<i64>,   // lookup em bgp_communities
    pub peer_id: Option<i64>,        // None = todos os peers
    pub origin: String,              // "manual" | "anomaly_detector"
    pub origin_detail: Option<String>,
}

// Anuncia uma rota:
// 1. Resolve community e peer do banco
// 2. Monta o comando ExaBGP
// 3. Escreve no FIFO
// 4. Insere em bgp_announcements (withdrawn_at = NULL)
// Retorna o id do anúncio criado.
pub async fn announce(
    pool: &SqlitePool,
    pipe_path: &str,
    req: AnnounceRequest,
) -> anyhow::Result<i64>

// Retira um anúncio:
// 1. Lê o anúncio do banco pelo id
// 2. Monta o comando withdraw
// 3. Escreve no FIFO
// 4. Preenche withdrawn_at = now() no banco
pub async fn withdraw(
    pool: &SqlitePool,
    pipe_path: &str,
    announcement_id: i64,
) -> anyhow::Result<()>

// Re-anuncia todos os anúncios ativos (para restart recovery)
pub async fn reannounce_all(
    pool: &SqlitePool,
    pipe_path: &str,
) -> anyhow::Result<usize>  // retorna quantos foram re-anunciados

// Monta o comando ExaBGP a partir dos dados
fn build_announce_command(
    prefix: &str,
    next_hop: &str,
    community: Option<&str>,  // já no formato "65000:9999 no-export"
    neighbor_ip: Option<&str>,
) -> String
```

---

## Exemplos de comandos gerados

**Anúncio global com community:**
```
announce route 203.0.113.0/24 next-hop self community [65000:9999]
```

**Anúncio para peer específico com community múltipla:**
```
neighbor 10.0.0.1 announce route 203.0.113.0/24 next-hop self community [65000:100 65000:200 no-export]
```

**Withdraw global:**
```
withdraw route 203.0.113.0/24
```

**Withdraw para peer específico:**
```
neighbor 10.0.0.1 withdraw route 203.0.113.0/24
```

---

## Escrita no FIFO

O FIFO é write-only, append, com `O_NONBLOCK`. Se o ExaBGP não estiver rodando
(pipe sem leitores), a escrita falha com `ENXIO` — registrar como warning, não erro fatal.

```rust
fn write_to_pipe(pipe_path: &str, command: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::fs::OpenOptions;
    let mut f = OpenOptions::new()
        .write(true)
        .open(pipe_path)?;
    writeln!(f, "{}", command)?;
    Ok(())
}
```

---

## Idempotência

- Anunciar o mesmo prefixo duas vezes substitui o anúncio anterior no ExaBGP
  (BGP UPDATE sobrescreve). No banco, inserimos nova linha — a linha anterior
  fica com `withdrawn_at` preenchido antes de inserir a nova.
- Withdraw de um prefixo não anunciado é ignorado pelo ExaBGP silenciosamente.

---

## Integração futura com Anomaly Detector (Fase 6)

Quando o detector de anomalias (task 6.2) identificar um ataque, poderá chamar:

```rust
bgp_control::announce(pool, pipe_path, AnnounceRequest {
    prefix: format!("{}/32", src_ip),
    next_hop: "self".to_string(),
    community_id: Some(blackhole_community_id),
    peer_id: None,  // todos os peers
    origin: "anomaly_detector".to_string(),
    origin_detail: Some(format!("alert_event#{}", event_id)),
}).await?;
```

---

## Aceite

- [ ] `announce` persiste no banco e escreve no FIFO com formato correto
- [ ] `withdraw` atualiza `withdrawn_at` e escreve no FIFO
- [ ] `reannounce_all` re-envia todos os ativos; retorna contagem
- [ ] Peer específico gera prefixo `neighbor <ip>` no comando
- [ ] Community `None` omite o atributo do comando
- [ ] FIFO ausente (ExaBGP offline) gera warning, não panic
- [ ] Mesmo prefixo anunciado duas vezes: banco fecha linha anterior, abre nova
