# Task 16.1 — Observabilidade fina no NOC

**Phase:** 16 (Observabilidade)
**Files:** `metrics/`, `collector-core/src/main.rs`, `collector-core/src/stats.rs`, `frontend/src/views/Dashboard.vue`

Quatro adições pedidas pelo Marcio (01/08/2026):

- **Lag de telemetria por exporter**: worker acumula `chegada − flowEnd` por
  exporter e publica `exporter_telemetry_lag_seconds{exporter_ip}` no flush;
  overview expõe o máximo; tile Exporters mostra "lag ~Ns". Diagnóstico direto
  de active timeout mal configurado no roteador.
- **Taxa de perda do coletor**: `packets_dropped/packets_received` (acumulado)
  no overview; no tile Exporters, vermelho acima de 1%.
- **Sparkline de alertas 24h**: 24 barras por hora (SQLite `alert_events`) no
  tile Alertas.
- **Top ASNs de destino**: card no NOC com top 5 por bytes (60 min,
  `direction=dst`) e barras proporcionais — visão de peering; clica → ASN Traffic.
