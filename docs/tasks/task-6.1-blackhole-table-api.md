# ~~Task 6.1 — Blackhole Rules Table + CRUD API~~ DEPRECATED

> **DEPRECATED** — Este arquivo descreve um design antigo baseado em GoBGP (`blackhole_rules` table,
> `blackhole_tx` watch channel). Foi substituído pela Fase 8 (ExaBGP) que usa as tabelas
> `bgp_peers`, `bgp_communities`, `bgp_prefixes` e `bgp_announcements` no SQLite.
>
> A nova task de infraestrutura BGP é `tasks/task-8.1-bgp-schema.md`.
> O endpoint equivalente (anunciar/retirar prefixo) é `POST/DELETE /api/bgp/announcements`
> documentado em `tasks/task-8.5-bgp-api.md` e em `03-api.md` (seção BGP).
>
> Manter este arquivo apenas para referência histórica. **Não implementar.**

---

## Problem (histórico — não implementar)

There is no persistence layer for blackhole rules. Rules need to survive restarts,
be visible in the dashboard, and be the single source of truth for what GoBGP announces.

## SQLite Table

Add to `collector-core/src/blackhole.rs`:

```sql
CREATE TABLE IF NOT EXISTS blackhole_rules (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    prefix       TEXT NOT NULL UNIQUE,      -- e.g. "203.0.113.5/32"
    reason       TEXT,                       -- "auto:pps" | "auto:bps" | "manual"
    source       TEXT NOT NULL DEFAULT 'manual',  -- "auto" | "manual"
    announced_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    expires_at   DATETIME,                  -- NULL = no expiry
    active       BOOLEAN NOT NULL DEFAULT 1
)
```

Create this table inside `init_blackhole_table(pool: &SqlitePool)` called at startup in `main.rs`.

## Structs

```rust
#[derive(Serialize, Deserialize, FromRow, Clone)]
pub struct BlackholeRule {
    pub id: i64,
    pub prefix: String,
    pub reason: Option<String>,
    pub source: String,
    pub announced_at: String,
    pub expires_at: Option<String>,
    pub active: bool,
}

#[derive(Deserialize)]
pub struct CreateBlackholeRequest {
    pub prefix: String,           // must be valid CIDR
    pub reason: Option<String>,
    pub expires_minutes: Option<u32>,  // None = no expiry
}
```

## Validation

In `create_blackhole_handler`, validate `prefix` using:
```rust
prefix.parse::<ipnetwork::IpNetwork>().map_err(|_| StatusCode::BAD_REQUEST)?;
```

Add `ipnetwork = "0.20"` to `collector-core/Cargo.toml`.

## Endpoints (Admin only)

```
GET    /api/blackhole           → list all active rules
POST   /api/blackhole           → add rule (triggers announcement — task 6.3)
DELETE /api/blackhole/:id       → deactivate + withdraw (task 6.3)
GET    /api/blackhole/config    → get anomaly thresholds
PUT    /api/blackhole/config    → update anomaly thresholds
```

For now, `POST` and `DELETE` only write to SQLite. The actual BGP announcement is wired in task 6.3.

## Expiry cleanup

Spawn a tokio task at startup that runs every 60 seconds:

```rust
async fn expire_blackholes(pool: SqlitePool) {
    loop {
        sqlx::query(
            "UPDATE blackhole_rules SET active = 0
             WHERE active = 1 AND expires_at IS NOT NULL AND expires_at < CURRENT_TIMESTAMP"
        )
        .execute(&pool).await.ok();
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
```

## AppState update

Add `blackhole_tx: tokio::sync::watch::Sender<Vec<BlackholeRule>>` to `AppState`.
This watch channel is how the anomaly detector (task 6.2) and bgp-controller (task 6.3) receive
rule change notifications without polling.

## Acceptance Criteria

- `GET /api/blackhole` returns active rules sorted by `announced_at DESC`
- `POST /api/blackhole` with `"203.0.113.5/32"` creates a rule and returns 201
- `POST /api/blackhole` with an invalid CIDR returns 400
- `DELETE /api/blackhole/:id` sets `active = 0`
- Expired rules (`expires_at < now`) are deactivated within 60 seconds
- `cargo build` succeeds
