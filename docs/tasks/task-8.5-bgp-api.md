# Task 8.5 — REST API BGP

**Status:** 🔲 A implementar  
**Fase:** 8 — BGP Announcement via ExaBGP  
**Depende de:** task 8.1, task 8.2, task 8.3, task 8.4

---

## Objetivo

Expor todos os recursos BGP via REST API. Todos os endpoints requerem admin.

---

## Endpoints

### Peers

#### `GET /api/bgp/peers`
Lista todos os peers. `md5_password` é retornado como `"***"` se preenchido (nunca expõe o valor real).

**Response 200:**
```json
[
  {
    "id": 1,
    "name": "Upstream Tier-1",
    "description": "Peer principal",
    "neighbor_ip": "10.0.0.1",
    "local_ip": "10.0.0.2",
    "local_as": 65001,
    "peer_as": 65000,
    "hold_time": 90,
    "has_md5": true,
    "enabled": true,
    "created_at": "2026-01-01T00:00:00"
  }
]
```

#### `POST /api/bgp/peers`
Cria peer. **Não** aplica config automaticamente — requer chamada a `POST /api/bgp/apply`.

**Request:**
```json
{
  "name": "Upstream Tier-1",
  "description": "opcional",
  "neighbor_ip": "10.0.0.1",
  "local_ip": "10.0.0.2",
  "local_as": 65001,
  "peer_as": 65000,
  "hold_time": 90,
  "md5_password": "opcional-ou-null",
  "enabled": true
}
```

**Response 201:** Peer criado (sem md5)  
**Response 409:** `neighbor_ip` já existe

#### `PUT /api/bgp/peers/:id`
Atualização completa do peer.  
**Response 200:** Peer atualizado  
**Response 404:** Peer não encontrado

#### `DELETE /api/bgp/peers/:id`
Remove peer. Falha se houver anúncios ativos referenciando este peer.  
**Response 204:** Removido  
**Response 409:** `{ "error": "Peer tem anúncios ativos. Retire os anúncios antes de remover." }`

---

### Communities

#### `GET /api/bgp/communities`
**Response 200:**
```json
[
  {
    "id": 1,
    "name": "Blackhole RTBH",
    "community": "65000:9999",
    "description": "RFC 5635 blackhole via upstream"
  }
]
```

#### `POST /api/bgp/communities`
```json
{ "name": "Blackhole RTBH", "community": "65000:9999", "description": "..." }
```
**Response 201:** Community criada  
**Response 409:** `community` já existe

#### `PUT /api/bgp/communities/:id`
**Response 200:** Community atualizada

#### `DELETE /api/bgp/communities/:id`
Falha se houver anúncios ativos usando esta community.  
**Response 204:** Removida  
**Response 409:** `{ "error": "Community em uso por anúncios ativos." }`

---

### Prefixes (catálogo)

#### `GET /api/bgp/prefixes`
```json
[
  { "id": 1, "prefix": "203.0.113.0/24", "description": "Bloco cliente A" }
]
```

#### `POST /api/bgp/prefixes`
```json
{ "prefix": "203.0.113.0/24", "description": "opcional" }
```
**Response 201:** Prefixo criado  
**Response 400:** Formato CIDR inválido  
**Response 409:** Já existe

#### `PUT /api/bgp/prefixes/:id`
**Response 200:** Atualizado

#### `DELETE /api/bgp/prefixes/:id`
**Response 204:** Removido (não verifica anúncios — prefixo no catálogo ≠ anúncio ativo)

---

### Anúncios (announce / withdraw)

#### `GET /api/bgp/announcements`
Lista anúncios. Query params:
- `active=true` — somente com `withdrawn_at IS NULL` (padrão)
- `active=false` — histórico completo

**Response 200:**
```json
[
  {
    "id": 5,
    "prefix": "203.0.113.0/24",
    "next_hop": "self",
    "community_name": "Blackhole RTBH",
    "community_value": "65000:9999",
    "peer_name": null,
    "peer_neighbor_ip": null,
    "origin": "manual",
    "origin_detail": null,
    "announced_at": "2026-07-08T10:00:00",
    "withdrawn_at": null
  }
]
```

#### `POST /api/bgp/announcements`
Anuncia uma rota. Persiste no banco e escreve no FIFO.

**Request:**
```json
{
  "prefix": "203.0.113.0/24",
  "next_hop": "self",
  "community_id": 1,
  "peer_id": null,
  "origin": "manual"
}
```

**Response 201:**
```json
{ "id": 5, "command": "announce route 203.0.113.0/24 next-hop self community [65000:9999]" }
```
O campo `command` é o texto exato enviado ao ExaBGP — útil para debug.

**Response 400:** Prefixo inválido, community_id ou peer_id inexistente  
**Response 503:** `{ "error": "ExaBGP pipe unavailable" }` — FIFO não está aberto

#### `DELETE /api/bgp/announcements/:id`
Retira um anúncio. Preenche `withdrawn_at` no banco e escreve `withdraw route` no FIFO.

**Response 200:** `{ "command": "withdraw route 203.0.113.0/24" }`  
**Response 404:** Anúncio não encontrado  
**Response 409:** `{ "error": "Anúncio já retirado" }`

---

### Config (apply)

#### `POST /api/bgp/apply`
Regenera o `exabgp.conf` a partir do banco e sinaliza reload do ExaBGP.
Necessário após criar/editar/deletar peers (mudanças de sessão).
**Não** necessário para announce/withdraw (escrita direta no FIFO).

**Response 200:**
```json
{
  "peers_configured": 2,
  "config_written": "/run/exabgp-config/exabgp.conf",
  "reload_signal": "SIGUSR1"
}
```

**Response 400:** Nenhum peer enabled configurado  
**Response 500:** Erro ao escrever arquivo ou sinalizar reload

---

### Sessions (status)

#### `GET /api/bgp/sessions`
Estado atual das sessões (lido do cache em memória — sem I/O).

**Response 200:**
```json
[
  {
    "peer_id": 1,
    "peer_name": "Upstream Tier-1",
    "neighbor_ip": "10.0.0.1",
    "state": "up",
    "last_up": "2026-07-08T09:00:00",
    "last_down": "2026-07-08T08:55:00",
    "updated_at": "2026-07-08T09:00:00"
  }
]
```

Estados possíveis: `"up"` | `"down"` | `"unknown"`

---

## Rotas no `main.rs`

Todas sob `require_admin`:

```rust
// Peers
.route("/api/bgp/peers", get(bgp::list_peers).post(bgp::create_peer))
.route("/api/bgp/peers/:id", put(bgp::update_peer).delete(bgp::delete_peer))
// Communities
.route("/api/bgp/communities", get(bgp::list_communities).post(bgp::create_community))
.route("/api/bgp/communities/:id", put(bgp::update_community).delete(bgp::delete_community))
// Prefixes
.route("/api/bgp/prefixes", get(bgp::list_prefixes).post(bgp::create_prefix))
.route("/api/bgp/prefixes/:id", put(bgp::update_prefix).delete(bgp::delete_prefix))
// Announcements
.route("/api/bgp/announcements", get(bgp::list_announcements).post(bgp::announce_route))
.route("/api/bgp/announcements/:id", delete(bgp::withdraw_route))
// Apply config
.route("/api/bgp/apply", post(bgp::apply_config))
// Sessions
.route("/api/bgp/sessions", get(bgp::get_sessions))
```

---

## Aceite

- [ ] CRUD de peers, communities, prefixes funcionando
- [ ] `POST /api/bgp/announcements` escreve no FIFO e retorna o `command`
- [ ] `DELETE /api/bgp/announcements/:id` retira e registra `withdrawn_at`
- [ ] `POST /api/bgp/apply` gera conf e retorna número de peers configurados
- [ ] `GET /api/bgp/sessions` responde sem I/O (cache em memória)
- [ ] `md5_password` nunca aparece em resposta de GET
- [ ] Delete de peer com anúncios ativos retorna 409
