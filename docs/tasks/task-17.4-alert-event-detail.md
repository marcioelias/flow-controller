# Task 17.4 — Detalhe do evento de alerta

**Phase:** 17 (Drill-down por IP)
**Files:** `collector-core/src/alert_api.rs`, `collector-core/src/main.rs`,
`frontend/src/stores/alerts.ts`, `frontend/src/views/AlertEvents.vue`

## Problema

Pedido do Marcio (03/10/2026): na tela de Eventos de Alerta não há como ver os
detalhes de um evento ao clicar nele. A mensagem fica truncada e a regra que disparou,
os parâmetros e o contexto não aparecem.

Achados na revisão da tela:

- **Selo BGP nunca aparece.** `list_events` não seleciona `bgp_announced`; o campo
  cai no default `false`.
- **Taxa de upload/download ~10× maior.** `upload_bytes`/`download_bytes` são somados
  na janela curta da regra (`short_window_min`, padrão 10 min — `detector.rs`), mas a
  tela divide por 60 s.
- Filtro `severity` é interpolado no SQL (só remove aspas) — vira lista branca.

## Regras

- **R-01** — Clicar numa linha abre um modal de detalhe (mesmo visual do modal de
  anomalia da task 15.1); Esc ou clique fora fecha.
- **R-02** — O modal mostra: severidade, tipo em português, hora; exporter (nome + IP);
  IP com atalho "Analisar tráfego" → `/talkers/:ip` (task 17.1); regra (nome, tipo,
  parâmetros chave → valor; "regra removida" se `rule_id` nulo); notificado (Telegram)
  sim/não; BGP anunciado sim/não; mensagem completa.
- **R-03** — Métricas por tipo:
  - `upload_inversion`: upload e download como **taxa média na janela da regra**
    (`bytes * 8 / (short_window_min * 60)`) e a razão download/upload.
  - `attack_signature`: PPS, pacote médio, portas atacadas.
  - `ml_anomaly`: PPS, pacote médio, upload/download (volume da janela, rotulado),
    explicação IA e feedback do operador, com atalho para a tela de IA.
- **R-04** — A coluna Upload/Download da tabela usa a mesma regra de taxa da R-03.
  Sem janela conhecida (regra removida), mostra volume com rótulo.
- **R-05** — `severity` aceita só `warning|critical`; outro valor é ignorado.
- **R-06** — Ordenação no servidor (task 17.3 R-05): `sort` ∈ {`created_at`,
  `severity`, `alert_type`, `exporter_ip`, `src_ip`, `notified`, `bgp_announced`},
  `dir` ∈ {`asc`,`desc`}; padrão `created_at desc`.

## API

- `GET /api/alerts/events` — cada evento ganha `window_min` (de
  `json_extract(params, '$.short_window_min')` da regra; nulo se não houver) e
  `bgp_announced` passa a vir do banco. Novos params `sort`, `dir`.
- `GET /api/alerts/events/:id` (NEW) — o evento + `window_min`, `explanation`,
  `feedback`, `exporter_name` e `rule: { id, name, rule_type, params } | null`.
  404 se não existir.

## Aceite

- AC-01: evento com `bgp_announced = 1` mostra o selo BGP na tabela e no modal.
- AC-02: `upload_inversion` com 750 MB em janela de 10 min mostra 10 Mbps (não 100 Mbps).
- AC-03: `GET /api/alerts/events/999999` → 404.
- AC-04: `severity=x' OR 1=1` é ignorado.
