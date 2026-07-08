# Task 8.6 — Frontend BGP

**Status:** 🔲 A implementar  
**Fase:** 8 — BGP Announcement via ExaBGP  
**Depende de:** task 8.5 (API completa)

---

## Objetivo

Implementar as views Vue 3 para gerenciamento BGP: sessões (status), anúncios ativos,
peers, communities e prefixos. Tudo restrito a admin.

---

## Arquivos a criar

| Arquivo | Rota | Descrição |
|---------|------|-----------|
| `src/stores/bgp.ts` | — | Pinia store unificado para todos os recursos BGP |
| `src/views/BgpDashboard.vue` | `/bgp` | Visão geral: sessões + anúncios ativos |
| `src/views/BgpPeers.vue` | `/bgp/peers` | CRUD de peers + botão "Aplicar Config" |
| `src/views/BgpCommunities.vue` | `/bgp/communities` | CRUD de communities |
| `src/views/BgpPrefixes.vue` | `/bgp/prefixes` | CRUD do catálogo de prefixos |
| `src/views/BgpAnnouncements.vue` | `/bgp/announcements` | Anúncios ativos + form de anúncio |

---

## Store: `src/stores/bgp.ts`

```typescript
interface BgpPeer { id, name, description, neighbor_ip, local_ip, local_as, peer_as, hold_time, has_md5, enabled }
interface BgpCommunity { id, name, community, description }
interface BgpPrefix { id, prefix, description }
interface BgpAnnouncement { id, prefix, next_hop, community_name, community_value, peer_name, peer_neighbor_ip, origin, origin_detail, announced_at, withdrawn_at }
interface BgpSession { peer_id, peer_name, neighbor_ip, state, last_up, last_down, updated_at }

// Actions:
loadSessions()         → GET /api/bgp/sessions
loadPeers()            → GET /api/bgp/peers
createPeer(data)       → POST /api/bgp/peers
updatePeer(id, data)   → PUT /api/bgp/peers/:id
deletePeer(id)         → DELETE /api/bgp/peers/:id
applyConfig()          → POST /api/bgp/apply

loadCommunities()      → GET /api/bgp/communities
createCommunity(data)  → POST /api/bgp/communities
updateCommunity(...)   → PUT /api/bgp/communities/:id
deleteCommunity(id)    → DELETE /api/bgp/communities/:id

loadPrefixes()         → GET /api/bgp/prefixes
createPrefix(data)     → POST /api/bgp/prefixes
updatePrefix(...)      → PUT /api/bgp/prefixes/:id
deletePrefix(id)       → DELETE /api/bgp/prefixes/:id

loadAnnouncements()    → GET /api/bgp/announcements
announce(data)         → POST /api/bgp/announcements
withdraw(id)           → DELETE /api/bgp/announcements/:id
```

---

## View: `BgpDashboard.vue` (`/bgp`)

Visão geral em duas seções, auto-refresh de sessões a cada 10s.

**Seção: Sessões BGP**
- Card por peer, badge colorido de estado:
  - `up` → verde (`bg-emerald-500/10 text-emerald-400`)
  - `down` → vermelho (`bg-red-500/10 text-red-400`)
  - `unknown` → cinza
- Exibe: peer name, neighbor IP, ASN remoto, last_up, last_down
- Botão "Gerenciar Peers" → `/bgp/peers`

**Seção: Anúncios Ativos**
- Tabela: Prefixo | Next-hop | Community | Peer (ou "Todos") | Origem | Anunciado às | [Retirar]
- Botão "Retirar" por linha: confirmação inline ("Confirmar?"), então `DELETE`
- Botão "Novo Anúncio" abre modal

**Modal "Novo Anúncio":**
```
Prefixo: [input text ou select do catálogo]
Next-hop: [input — default "self"]
Community: [select das communities cadastradas, ou "Nenhuma"]
Peer: [select dos peers, ou "Todos os peers"]
```
Botão "Anunciar" → `POST /api/bgp/announcements` → fecha modal, recarrega lista.
Exibe o `command` retornado em caixa mono por 5s ("Comando enviado ao ExaBGP").

---

## View: `BgpPeers.vue` (`/bgp/peers`)

- Tabela: Nome | Neighbor IP | Local IP | ASN local | ASN remoto | Hold time | MD5 | Status | Ações
- Botões por linha: Editar | Desativar/Ativar | Excluir
- Formulário inline (ou modal) para criar/editar peer
- **Botão "Aplicar Configuração"** — destaque visual (amarelo/âmbar), com aviso:
  > "Isso regenera o exabgp.conf e reinicia as sessões BGP. Confirmar?"
  - Após aplicar: exibe resultado com número de peers configurados

---

## View: `BgpCommunities.vue` (`/bgp/communities`)

- Tabela simples: Nome | Community | Descrição | Ações (Editar | Excluir)
- Formulário inline para criar
- Campo `community` aceita múltiplos valores (`65000:100 no-export`)
- Badge colorido por nome para facilitar reconhecimento visual

---

## View: `BgpPrefixes.vue` (`/bgp/prefixes`)

- Catálogo de prefixos gerenciáveis
- Tabela: Prefixo | Descrição | Ações (Editar | Excluir | Anunciar)
- Botão "Anunciar" por linha: abre modal pré-preenchido com o prefixo
- Validação CIDR básica no frontend (regex: `^\d+\.\d+\.\d+\.\d+/\d+$`)

---

## View: `BgpAnnouncements.vue` (`/bgp/announcements`)

- Histórico completo (inclui retirados)
- Filtro: "Somente ativos" (default on)
- Anúncios ativos com badge verde, retirados com badge cinza e `withdrawn_at`
- Coluna "Origem" — `manual` ou `anomaly_detector` (com ícone diferente para automáticos)

---

## Router e Sidebar

**Rotas no `router.ts`:**
```typescript
{ path: '/bgp',               name: 'BgpDashboard',    component: () => import('./views/BgpDashboard.vue'),    meta: { requiresAuth: true, requiresAdmin: true } },
{ path: '/bgp/peers',         name: 'BgpPeers',        component: () => import('./views/BgpPeers.vue'),        meta: { requiresAuth: true, requiresAdmin: true } },
{ path: '/bgp/communities',   name: 'BgpCommunities',  component: () => import('./views/BgpCommunities.vue'),  meta: { requiresAuth: true, requiresAdmin: true } },
{ path: '/bgp/prefixes',      name: 'BgpPrefixes',     component: () => import('./views/BgpPrefixes.vue'),     meta: { requiresAuth: true, requiresAdmin: true } },
{ path: '/bgp/announcements', name: 'BgpAnnouncements',component: () => import('./views/BgpAnnouncements.vue'),meta: { requiresAuth: true, requiresAdmin: true } },
```

**Sidebar (`AppLayout.vue`)** — nova seção "BGP" entre Análise e Administração:
```
[Radio] BGP
  ├── [Activity] Sessões / Anúncios   → /bgp
  ├── [Server]   Peers                → /bgp/peers
  ├── [Tag]      Communities          → /bgp/communities
  ├── [List]     Prefixos             → /bgp/prefixes
  └── [Clock]    Histórico            → /bgp/announcements
```

Ícones Lucide sugeridos: `Radio`, `Activity`, `Server`, `Tag`, `ListFilter`, `History`.

---

## Aceite

- [ ] Sessões atualizam automaticamente a cada 10s no BgpDashboard
- [ ] Novo anúncio via modal exibe o `command` ExaBGP por 5s
- [ ] Retirar anúncio tem confirmação inline
- [ ] Botão "Aplicar Configuração" em Peers tem aviso de impacto
- [ ] Prefixos têm botão "Anunciar" que pré-preenche o modal
- [ ] Histórico de anúncios distingue visualmente ativo vs. retirado vs. originado por detector
- [ ] Todas as rotas `/bgp/*` redirecionam para `/dashboard` para não-admins
