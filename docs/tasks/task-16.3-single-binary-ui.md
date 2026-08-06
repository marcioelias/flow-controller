# Task 16.3 — SPA embutida no binário + superfície do ClickHouse 🔶 parcial

**Phase:** 16 (Observabilidade / Distribuição)

## Achado de segurança (01/08/2026) — CORRIGIDO

O nginx do dashboard proxeava `/ch-api/*` direto para o ClickHouse **sem
autenticação** (e removia o header Authorization de propósito). Nenhum código do
frontend usava mais a rota — era superfície de ataque vestigial das fases 1–9.

Correção aplicada:
- Bloco `/ch-api` removido do nginx.conf e do proxy do vite
- Portas 8123/9000 do compose agora bind em `127.0.0.1` (o CH roda sem senha —
  nunca expor na LAN; dentro do compose a rede interna resolve)

## Pendente — embed da SPA (nível 16.3 completo)

- `rust-embed` do `dist/` no binário; axum serve UI + API + WS numa porta só
- Elimina o container/dependência de nginx (Docker e .deb)
- nginx opcional apenas para TLS na frente
