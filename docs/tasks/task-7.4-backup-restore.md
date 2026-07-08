# Task 7.4 — Backup & Restore

## Status: ✅ IMPLEMENTADO

## Objetivo

Permitir exportar toda a configuração do collector (usuários, exporters, licença) para um
arquivo JSON portável, e importar em uma nova instalação. Opcionalmente, incluir os flows
coletados (com aviso de tamanho antes do download).

---

## Escopo do backup

### Config backup (sempre incluído)

| Dado | Fonte | Formato no backup |
|------|-------|-------------------|
| Usuários | SQLite `users` | JSON array (senha como hash bcrypt — não em claro) |
| Exporters | SQLite `exporters` | JSON array completo |
| Licença atual | `license.key` em disco | String raw do arquivo |

### Flows backup (opcional, sob alerta)

| Dado | Fonte | Formato |
|------|-------|---------|
| `network_flows_v4` | ClickHouse | NDJSON ou CSV comprimido (gz) |
| `network_flows_v6` | ClickHouse | Idem |

Antes de iniciar o download dos flows, o backend retorna o tamanho estimado e o frontend
exibe um modal de confirmação: "O export de flows tem ~X GB. Deseja continuar?"

---

## API

### GET /api/backup/config

Retorna o backup de configuração como download JSON.

Auth: admin only.

**Response:** `Content-Disposition: attachment; filename="flow-collector-config-<date>.json"`

```json
{
  "version": 1,
  "exported_at": "2026-07-08T10:00:00Z",
  "users": [
    { "username": "admin", "password_hash": "$2b$12$...", "is_admin": true, "created_at": "..." }
  ],
  "exporters": [
    { "ip_address": "10.0.1.1", "name": "BNG-01", "description": "...", "location": "...", "enabled": true }
  ],
  "license": "eyJmaW5nZXJwcmludCI6..."
}
```

**Nota sobre senhas:** os hashes bcrypt são exportados, não as senhas em claro. Na
importação, os hashes são reusados diretamente — o usuário não precisa resetar senhas.
A licença é incluída mas será inválida na nova máquina (fingerprint diferente) — serve
apenas de referência para solicitar nova licença.

### GET /api/backup/flows/size

Retorna o tamanho estimado do export de flows sem fazer o download.

Auth: admin only.

```json
{
  "v4_rows": 125000000,
  "v6_rows": 8000000,
  "estimated_bytes_uncompressed": 17200000000,
  "estimated_bytes_compressed": 2150000000,
  "warning": "O export comprimido tem ~2.0 GB. O processo pode levar vários minutos."
}
```

Query ClickHouse para estimar:
```sql
SELECT count() AS rows,
       sum(bytes) * 2 AS estimated_bytes   -- fator 2× overhead conservador
FROM network_flows_v4
```

### GET /api/backup/flows

Inicia o streaming download dos flows como NDJSON comprimido com gzip.

Auth: admin only.

**Response:** `Content-Disposition: attachment; filename="flows-<date>.ndjson.gz"`
`Content-Type: application/gzip`

Streaming via Axum `Body::from_stream` — não carrega tudo em memória.

Query ClickHouse com `FORMAT JSONEachRow` para NDJSON nativo.

### POST /api/restore/config

Importa um arquivo de config JSON.

Auth: admin only.

**Request:** `multipart/form-data` com campo `file` contendo o JSON.

**Comportamento:**
- Usuários: `INSERT OR IGNORE` — não sobrescreve usuários existentes (evita conflito com
  o admin atual). Retorna lista de usuários importados vs ignorados.
- Exporters: `INSERT OR REPLACE` — sobrescreve pelo `ip_address` como chave única.
- Licença: salva o arquivo em disco para referência, mas **não aplica** (fingerprint
  diferente) — exibe aviso na resposta.

**Response 200:**
```json
{
  "users_imported": 3,
  "users_skipped": 1,
  "exporters_imported": 12,
  "exporters_replaced": 2,
  "license": "skipped — license is bound to original machine fingerprint"
}
```

**Response 400:** JSON inválido ou `version` incompatível.

---

## Frontend

### Nova view: `frontend/src/views/BackupRestore.vue`

```
┌─────────────────────────────────────────────────────────────────┐
│  Backup & Restauração                                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ── Exportar ──────────────────────────────────────────────── │
│                                                                 │
│  [⬇ Baixar configuração]   (users, exporters, licença)         │
│                                                                 │
│  [⬇ Baixar flows]          (verifica tamanho primeiro)         │
│                                                                 │
│  ── Importar ──────────────────────────────────────────────── │
│                                                                 │
│  Arraste ou selecione o arquivo de backup:                     │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │           📂  flow-collector-config-2026-07-08.json     │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  [Importar configuração]                                        │
│                                                                 │
│  ✓ 3 usuários importados, 1 ignorado (já existia)              │
│  ✓ 12 exporters importados                                      │
│  ⚠ Licença não importada (vinculada a outro hardware)          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**"Baixar flows" flow:**
1. Clique → faz GET `/api/backup/flows/size`
2. Exibe modal: "O export tem ~2.0 GB comprimido. Isso pode levar vários minutos. Continuar?"
3. Confirmar → `window.location.href = '/api/backup/flows'` (browser faz o download)

**Import:**
- `<input type="file" accept=".json">` com drag-and-drop
- Valida `version` e `exported_at` antes de enviar
- Exibe resultado linha a linha com ícones ✓/⚠

### Router

```ts
{ path: '/backup', name: 'Backup',
  component: () => import('./views/BackupRestore.vue'),
  meta: { requiresAuth: true, requiresAdmin: true } }
```

### Sidebar

Dentro do bloco admin, após "Usuários":

```html
<router-link to="/backup" ...>
  <DatabaseBackup class="w-5 h-5" />
  Backup
</router-link>
```

---

## Acceptance criteria

- [ ] `cargo build` passa
- [ ] GET `/api/backup/config` retorna JSON válido com todos os campos
- [ ] Senhas nunca aparecem em claro no backup
- [ ] GET `/api/backup/flows/size` retorna estimativa sem timeout
- [ ] GET `/api/backup/flows` faz streaming (não OOM com tabelas grandes)
- [ ] POST `/api/restore/config` com JSON da v1 importa sem erros
- [ ] Usuário existente (mesmo username) é ignorado na importação
- [ ] Importar licença não a aplica — exibe aviso
- [ ] Frontend exibe modal de confirmação antes de iniciar download de flows
- [ ] Frontend exibe resultado da importação com contagens
