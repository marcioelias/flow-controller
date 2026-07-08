# Task 8.4 — Monitor de Sessão BGP (Background Task)

**Status:** 🔲 A implementar  
**Fase:** 8 — BGP Announcement via ExaBGP  
**Depende de:** task 8.2 (FIFO e container ExaBGP rodando)

---

## Objetivo

Implementar uma task async que consome o FIFO de saída do ExaBGP (`exabgp.out`)
para detectar mudanças de estado das sessões (UP/DOWN) e mantém uma tabela de
sessões atual em memória e no SQLite.

---

## Como o ExaBGP reporta eventos de sessão

Quando `encoder text` está configurado, o ExaBGP escreve linhas no STDOUT do
processo controlador (que é o próprio `exabgp.out`). Os eventos de neighbor state são:

```
neighbor 10.0.0.1 up
neighbor 10.0.0.1 down
neighbor 10.0.0.1 connected
```

Para o ExaBGP enviar para o FIFO de saída, o conf usa:

```
process flowvision-controller {
    run /bin/sh -c 'cat /run/exabgp/exabgp.in';  # lê comandos
    encoder text;
}
```

E o ExaBGP escreve os eventos no STDOUT do processo, que redirecionamos para
`/run/exabgp/exabgp.out`.

---

## Tabela SQLite: `bgp_sessions`

Mantém o estado atual de cada peer. Uma linha por peer.

```sql
CREATE TABLE IF NOT EXISTS bgp_sessions (
    peer_id      INTEGER PRIMARY KEY REFERENCES bgp_peers(id) ON DELETE CASCADE,
    state        TEXT NOT NULL DEFAULT 'unknown',  -- 'up' | 'down' | 'unknown'
    last_up      DATETIME,
    last_down    DATETIME,
    updated_at   DATETIME DEFAULT CURRENT_TIMESTAMP
)
```

Populada automaticamente pelo monitor ao criar peers (INSERT OR IGNORE).

---

## Estado em memória: `SessionCache`

Além do SQLite (persistência), mantemos um cache em memória para o endpoint de
status responder sem I/O:

```rust
// Em AppState
pub bgp_sessions: Arc<RwLock<HashMap<String, BgpSessionState>>>
    // key: neighbor_ip (String)

pub struct BgpSessionState {
    pub peer_id: i64,
    pub peer_name: String,
    pub neighbor_ip: String,
    pub state: String,       // "up" | "down" | "unknown"
    pub last_up: Option<String>,
    pub last_down: Option<String>,
    pub updated_at: String,
}
```

---

## Background task: `bgp_session_monitor`

```rust
pub async fn bgp_session_monitor(
    pool: SqlitePool,
    sessions: Arc<RwLock<HashMap<String, BgpSessionState>>>,
    out_pipe_path: String,   // "/run/exabgp/exabgp.out"
)
```

**Loop principal:**

1. Abre `exabgp.out` em modo leitura bloqueante
2. Lê linha por linha
3. Parseia eventos `neighbor <ip> <state>`:
   - `up` → atualiza `state`, `last_up`, `updated_at`
   - `down` → atualiza `state`, `last_down`, `updated_at`
4. Atualiza o cache em memória
5. Persiste no SQLite (`UPDATE bgp_sessions SET ... WHERE peer_id = ?`)
6. Se o pipe não existir (ExaBGP offline), aguarda 5s e tenta novamente

**Parser de linha:**

```
"neighbor 10.0.0.1 up"
 ^^^^^^^^ ^^^^^^^^^ ^^
 literal  ip         state
```

Ignorar linhas que não batem com o padrão (announcements, etc.).

---

## Inicialização do cache

Na startup do collector, antes de spawnar o monitor:

1. Ler todos os peers do banco
2. Para cada peer, fazer `INSERT OR IGNORE INTO bgp_sessions (peer_id, state) VALUES (?, 'unknown')`
3. Carregar todas as sessões do banco para o cache em memória

---

## Integração com AppState

```rust
pub struct AppState {
    // ... campos existentes
    pub bgp_sessions: Arc<std::sync::RwLock<
        std::collections::HashMap<String, bgp::BgpSessionState>
    >>,
    pub exabgp_pipe: String,         // path do FIFO de entrada
    pub exabgp_config_path: String,  // path do exabgp.conf
}
```

---

## Aceite

- [ ] Sessões de peers aparecem como `"unknown"` ao criar um peer
- [ ] Ao receber `"neighbor 10.0.0.1 up"` no pipe, cache e banco atualizam para `"up"`
- [ ] Ao receber `"neighbor 10.0.0.1 down"`, atualiza para `"down"` com `last_down`
- [ ] Restart do collector preserva `last_up` / `last_down` via SQLite
- [ ] Pipe ausente não causa panic — monitor aguarda e reabre em loop
- [ ] Cache é thread-safe (RwLock) e responde sem latência ao endpoint de status
