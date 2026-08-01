# Task 13.10 — Janela ao vivo persistente entre views

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 45m
**Files:** `frontend/src/composables/useLiveTraffic.ts`, `frontend/src/views/Dashboard.vue`

## Problema

Trocar de aba/view desmontava o Dashboard: o WebSocket fechava e o buffer de 5 min
morria junto — o gráfico de tempo real reiniciava vazio a cada navegação.

## Implementação

- `useLiveTraffic()`: estado em escopo de módulo — o WS conecta uma vez por sessão
  do SPA e **continua acumulando fatias com o Dashboard desmontado**; ao voltar, a
  janela está completa
- Snapshot em `sessionStorage` (throttle 5s): a janela sobrevive a um reload (F5);
  por aba do navegador, some ao fechar — comportamento de "sessão atual"
- O modo por dispositivo continua com buffer local (payload agregado por device
  não tem fatia/família) — reinicia ao navegar, registrado como limitação

## Aceite

- Dashboard → Histórico → Dashboard: gráfico volta com os 5 min completos
- F5: janela restaurada do snapshot
- Fechar a aba do navegador: janela zera (sessão nova)
