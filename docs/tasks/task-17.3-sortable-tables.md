# Task 17.3 — Ordenação por coluna em todas as tabelas

**Phase:** 17 (Drill-down por IP)
**Files:** `frontend/src/composables/useSort.ts` (NEW), `frontend/src/components/SortTh.vue` (NEW),
todas as views com `<table>`, `collector-core/src/alert_api.rs` (eventos)

## Problema

Pedido do Marcio (03/10/2026): "nas tabelas, sempre coloque ordenação ao clicar nas
colunas". Hoje nenhuma tabela ordena.

## Regras

- **R-01** — Toda tabela de dados ordena ao clicar no cabeçalho de uma coluna de dado.
  Colunas de ação (botões, toggles) não ordenam.
- **R-02** — 1º clique numa coluna: numéricas começam em ordem decrescente, texto em
  crescente; clique seguinte inverte. Indicador ▲/▼ na coluna ativa; as demais mostram
  um indicador neutro no hover.
- **R-03** — A ordem inicial é a que a tela já tinha (ex.: p95 desc nos rankings,
  hora desc nos eventos), marcada como coluna ativa.
- **R-04** — Comparação: números numericamente; texto com `localeCompare(…, 'pt-BR',
  { numeric: true })` (IPs e portas em texto ordenam natural); nulos sempre por último.
- **R-05** — Tabelas paginadas no servidor (Eventos de Alerta) ordenam no servidor:
  `GET /api/alerts/events` aceita `sort` (lista branca de colunas) e `dir` (`asc|desc`);
  valor fora da lista → ordem padrão (`created_at desc`). Trocar a ordenação volta à página 1.
- **R-06** — Tabelas novas nascem com ordenação (regra permanente do projeto).

## Infra

- `useSort(rows, initialKey, initialDir, getters?)` → `{ sorted, key, dir, toggle }`.
  `getters` permite ordenar colunas derivadas (ex.: nome do exporter a partir do IP).
- `<SortTh :sort="sort" k="campo" align="right">Rótulo</SortTh>` renderiza o `<th>`
  clicável com indicador e `aria-sort`.

## Tabelas

Dashboard (exporters), Top Talkers, ASN, Portas, Talker (conversas, portas),
Eventos de Alerta (servidor), Regras de Alerta, IA (2 tabelas), BGP (dashboard,
peers, prefixos, comunidades, anúncios), Usuários.

## Aceite

- AC-01: clicar duas vezes na mesma coluna inverte a ordem.
- AC-02: nas portas, ordenar por "Porta" ordena 22 < 443 < 8080 (numérico).
- AC-03: nos eventos, ordenar por severidade reconsulta o servidor e volta à página 1.
- AC-04: `sort=; DROP TABLE` é ignorado (ordem padrão), sem erro.
