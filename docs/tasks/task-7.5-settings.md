# Task 7.5 — Configurações do Sistema (Settings)

**Status:** ✅ Implementado  
**Fase:** 7 — Licensing, Health & Ops  
**Versão introduzida:** 1.1

---

## Objetivo

Expor configurações globais do sistema via tela administrativa, permitindo ao operador
ajustar parâmetros sem editar arquivos de configuração ou reiniciar o serviço
(exceto onde indicado). Keys são pré-definidas em código; o usuário só altera o `value`.

---

## Arquivos modificados / criados

| Arquivo | Ação |
|---------|------|
| `collector-core/src/settings.rs` | Novo módulo |
| `collector-core/src/main.rs` | `mod settings`, init, rotas, leitura de `COLLECTOR_WORKERS` |
| `frontend/src/stores/settings.ts` | Pinia store |
| `frontend/src/views/Settings.vue` | View com grupos e salvar individual |
| `frontend/src/router.ts` | Rota `/settings` (admin) |
| `frontend/src/layouts/AppLayout.vue` | Link "Configurações" no sidebar |

---

## Modelo de dados

```sql
CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL DEFAULT '',
    label       TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    group_name  TEXT NOT NULL DEFAULT ''
);
```

Seed via `INSERT OR IGNORE` no startup — preserva valores editados pelo usuário entre reinícios.

---

## Keys pré-definidas

| Key | Grupo | Padrão | Requer reinício |
|-----|-------|--------|-----------------|
| `OWN_ASN_LIST` | Rede | — | Não |
| `INTERNAL_PREFIXES` | Rede | — | Não |
| `ORG_NAME` | Organização | — | Não |
| `TIMEZONE` | Organização | `America/Sao_Paulo` | Não |
| `FLOW_RETENTION_DAYS` | Análise | `30` | **Sim** (recria TTL ClickHouse) |
| `MAX_TOP_TALKERS` | Análise | `50` | Não |
| `DEFAULT_WINDOW_MIN` | Análise | `60` | Não |
| `ALERT_COOLDOWN_MIN` | Alertas | `60` | Não |
| `UPLOAD_INVERSION_FACTOR` | Alertas | `2` | Não |
| `ATTACK_PPS_THRESHOLD` | Alertas | `10000` | Não |
| `ATTACK_PKT_SIZE_MAX` | Alertas | `200` | Não |
| `SESSION_TIMEOUT_HOURS` | Sessão | `24` | Não |
| `COLLECTOR_WORKERS` | Coletor | `4` | **Sim** (lido no startup) |

---

## API

Documentada em `docs/03-api.md` — seção Settings.

- `GET /api/settings` — auth (qualquer role)
- `PUT /api/settings/:key` — admin only; retorna 404 para keys fora da lista permitida

---

## Frontend

- Rota: `/settings` (admin only)
- View agrupa settings por `group_name` na ordem: Rede → Organização → Análise → Alertas → Sessão → Coletor
- Cada item: label, descrição, input de texto, botão Salvar (ativo apenas quando o valor foi alterado)
- Feedback visual por item: spinner → check verde (2.5s) → reset / X vermelho (3s) em caso de erro
- Enter no input dispara o save

---

## Helpers disponíveis para outros módulos

```rust
// Qualquer valor por key
settings::get_value(&pool, "KEY").await -> Option<String>

// Lista de ASNs próprios parseada como Vec<u32>
settings::get_own_asns(&pool).await -> Vec<u32>
```

A Fase 6 (anomaly detector) deve usar `get_own_asns` e `get_value` para
`UPLOAD_INVERSION_FACTOR`, `ATTACK_PPS_THRESHOLD`, `ATTACK_PKT_SIZE_MAX` e `ALERT_COOLDOWN_MIN`.

---

## Aceite

- [x] `GET /api/settings` retorna lista completa com grupos
- [x] `PUT /api/settings/OWN_ASN_LIST` persiste e retorna valor atualizado
- [x] `PUT /api/settings/CHAVE_INEXISTENTE` retorna 404
- [x] Reiniciar o processo usa o valor salvo de `COLLECTOR_WORKERS`
- [x] Seed `INSERT OR IGNORE` não sobrescreve valores editados
- [x] Frontend exibe grupos corretos com feedback de salvo/erro por linha
