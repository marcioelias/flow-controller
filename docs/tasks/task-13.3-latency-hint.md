# Task 13.3 — Indicador de latência de telemetria na UI

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 30m
**Files:** `frontend/src/views/Dashboard.vue`

## Problema

Flow telemetry tem atraso inerente: o exporter só envia o flow quando ele expira
(active timeout — 60s típico em roteador de produção). Operador olha o tile "último
minuto", não vê o pico que acabou de acontecer e conclui que a ferramenta falhou.

## Implementação

- Tooltip nos tiles ENTRADA/SAÍDA/PICO explicando: "Flows são recebidos quando o
  exporter os expira (active timeout). Picos recentes podem levar até o intervalo de
  timeout para aparecer."
- Com 13.2, os dados chegam retroativos — o tile de último minuto se preenche
  sozinho ao receber flows atrasados. O tooltip explica o porquê da atualização
  retroativa.

## Aceite

- Tooltip visível nos tiles de taxa
- Nenhuma mudança de layout
