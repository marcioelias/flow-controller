# Task 16.3 — SPA embutida no binário + superfície do ClickHouse ✅

**Phase:** 16 (Observabilidade / Distribuição)

## Achado de segurança (01/08/2026) — CORRIGIDO

O nginx do dashboard proxeava `/ch-api/*` direto para o ClickHouse **sem
autenticação** (e removia o header Authorization de propósito). Nenhum código do
frontend usava mais a rota — era superfície de ataque vestigial das fases 1–9.

Correção aplicada:
- Bloco `/ch-api` removido do nginx.conf e do proxy do vite
- Portas 8123/9000 do compose agora bind em `127.0.0.1` (o CH roda sem senha —
  nunca expor na LAN; dentro do compose a rede interna resolve)

## Embed implementado (01/08/2026)

- `rust-embed` embute o `dist/` no binário; fallback do axum serve a SPA com
  MIME correto e rota Vue caindo no index.html; caminhos `/api|/ws|/metrics`
  desconhecidos seguem 404
- Dockerfile em 3 estágios (node → rust → slim): a imagem do dashboard e o
  nginx **deixam de existir** — compose com 2 serviços essenciais (collector +
  clickhouse), UI publicada em `8080:3000`
- docker-publish só constrói a imagem do coletor; install.sh/update.sh sem
  referências à imagem antiga; CI cria placeholder do dist para clippy/test
- Em debug o rust-embed lê do disco — o fluxo vite de dev não muda
