# Task 8.1 — BGP Schema SQLite + Tipos Rust

**Status:** 🔲 A implementar  
**Fase:** 8 — BGP Announcement via ExaBGP  
**Depende de:** nenhuma (pode rodar antes das demais)

---

## Objetivo

Criar as tabelas SQLite que representam a configuração BGP (peers, communities, prefixes,
rotas anunciadas) e os tipos Rust correspondentes. Esta task é **somente dados** — nenhuma
integração com ExaBGP acontece aqui.

---

## Tabelas SQLite a criar em `auth.db`

### `bgp_peers`
Representa um vizinho BGP. Um peer define **com quem** o ExaBGP se conecta.

```sql
CREATE TABLE IF NOT EXISTS bgp_peers (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL,
    description   TEXT,
    neighbor_ip   TEXT NOT NULL UNIQUE,  -- IP do peer remoto (ex: "10.0.0.1")
    local_ip      TEXT NOT NULL,         -- IP local da interface de peering
    local_as      INTEGER NOT NULL,      -- ASN local (ex: 65001)
    peer_as       INTEGER NOT NULL,      -- ASN do peer (ex: 65000)
    hold_time     INTEGER NOT NULL DEFAULT 90,
    md5_password  TEXT,                  -- NULL = sem MD5
    enabled       BOOLEAN NOT NULL DEFAULT 1,
    created_at    DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at    DATETIME DEFAULT CURRENT_TIMESTAMP
)
```

### `bgp_communities`
Mapa de comunidades BGP com nome legível e finalidade.

```sql
CREATE TABLE IF NOT EXISTS bgp_communities (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,           -- ex: "Blackhole", "Mitigação RTBH"
    community   TEXT NOT NULL UNIQUE,    -- ex: "65000:9999" ou "65000:100 65000:200"
    description TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
)
```

> `community` é uma string livre — aceita múltiplos valores separados por espaço,
> exatamente como o ExaBGP espera: `"65000:100 no-export"`.

### `bgp_prefixes`
Prefixos que o operador pode anunciar/retirar. Funciona como "catálogo" de prefixos
gerenciáveis — não significa que estão sendo anunciados agora.

```sql
CREATE TABLE IF NOT EXISTS bgp_prefixes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    prefix      TEXT NOT NULL UNIQUE,    -- ex: "192.0.2.0/24"
    description TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
)
```

### `bgp_announcements`
Anúncios ativos. Cada linha = um prefixo sendo anunciado agora via ExaBGP.
É a fonte de verdade para o estado desejado — ao iniciar, o sistema re-anuncia tudo.

```sql
CREATE TABLE IF NOT EXISTS bgp_announcements (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    prefix         TEXT NOT NULL,         -- pode ser ad-hoc, não precisa estar em bgp_prefixes
    next_hop       TEXT NOT NULL,         -- ex: "192.0.2.254" ou "self"
    community_id   INTEGER REFERENCES bgp_communities(id) ON DELETE SET NULL,
    peer_id        INTEGER REFERENCES bgp_peers(id) ON DELETE CASCADE,
                                          -- NULL = anuncia para todos os peers
    origin         TEXT NOT NULL DEFAULT 'manual',  -- "manual" | "anomaly_detector"
    origin_detail  TEXT,                  -- ex: "alert_event#42"
    announced_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
    withdrawn_at   DATETIME             -- NULL = ativo; preenchido ao retirar
)
```

> Não deletamos linhas — preenchemos `withdrawn_at` para manter histórico de anúncios.

---

## Módulo Rust a criar: `collector-core/src/bgp.rs`

```rust
// Tipos mínimos (derivam de FromRow para sqlx)

pub struct BgpPeer {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub neighbor_ip: String,
    pub local_ip: String,
    pub local_as: i64,
    pub peer_as: i64,
    pub hold_time: i64,
    pub md5_password: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub struct BgpCommunity {
    pub id: i64,
    pub name: String,
    pub community: String,  // ex: "65000:9999" — usado diretamente no comando ExaBGP
    pub description: Option<String>,
    pub created_at: String,
}

pub struct BgpPrefix {
    pub id: i64,
    pub prefix: String,
    pub description: Option<String>,
    pub created_at: String,
}

pub struct BgpAnnouncement {
    pub id: i64,
    pub prefix: String,
    pub next_hop: String,
    pub community_id: Option<i64>,
    pub peer_id: Option<i64>,
    pub origin: String,
    pub origin_detail: Option<String>,
    pub announced_at: String,
    pub withdrawn_at: Option<String>,
}

// Usado para GET /api/bgp/announcements (JOIN com community e peer)
pub struct BgpAnnouncementView {
    pub id: i64,
    pub prefix: String,
    pub next_hop: String,
    pub community_name: Option<String>,
    pub community_value: Option<String>,
    pub peer_name: Option<String>,
    pub peer_neighbor_ip: Option<String>,
    pub origin: String,
    pub origin_detail: Option<String>,
    pub announced_at: String,
    pub withdrawn_at: Option<String>,
}
```

---

## Função de init

```rust
pub async fn init_bgp_tables(pool: &SqlitePool) -> anyhow::Result<()> {
    // CREATE TABLE IF NOT EXISTS para cada tabela acima
    // Sem seed — o operador preenche via UI
}
```

Chamada em `main.rs` após `init_settings_table`.

---

## Aceite

- [ ] Tabelas criadas no SQLite ao iniciar o serviço
- [ ] Structs derivam `Serialize`, `Deserialize`, `FromRow`
- [ ] `init_bgp_tables` é idempotente (IF NOT EXISTS)
- [ ] `bgp_announcements` com `withdrawn_at = NULL` representa estado ativo
- [ ] Compila sem warnings
