# Task 13.5 — Dashboard em abas

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 45m
**Files:** `frontend/src/router.ts`, `frontend/src/views/DashboardTabs.vue`, `frontend/src/layouts/AppLayout.vue`

## Problema

Dashboard, Histórico e Servidor eram três itens soltos no menu lateral — três
visões do mesmo assunto (estado da rede) espalhadas na navegação.

## Implementação

- Item único **Dashboard** no menu
- `DashboardTabs.vue`: barra de abas fixa (Tempo real / Histórico / Servidor),
  filhos renderizados via rotas aninhadas:
  - `/dashboard` → Tempo real (default)
  - `/dashboard/history` → Histórico
  - `/dashboard/server` → Servidor
- Rotas antigas `/history` e `/server` redirecionam (bookmarks continuam válidos)

## Aceite

- Menu com um único item Dashboard
- Aba ativa destacada; deep-link direto para cada aba funciona
- `/history` e `/server` redirecionam
