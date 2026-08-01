# Task 14.1 — Enforcement da licença (free tier) 🔲

**Phase:** 14 (Backlog)
**Effort:** a definir
**Files:** `collector-core/src/license.rs`, ponto de aplicação a decidir

## Problema (constatado em 31/07/2026)

`max_bps` e `max_talkers` são validados, persistidos e exibidos na tela de
Licença — e **nenhum código lê esses limites**. O free tier "100 Mbps / 1 talker"
é decorativo: o produto completo funciona sem licença.

## Decisão de produto pendente (Marcio)

Como o free tier deve se comportar? Opções, não exclusivas:

| Opção | Efeito | Esforço |
|-------|--------|---------|
| A. Limitar coleta | Acima de `max_bps` (média móvel), descartar flows e avisar na UI | médio |
| B. Limitar visão | Top Talkers/consultas mostram só `max_talkers` IPs; resto agregado como "Outros" | baixo |
| C. Degradar dados | Histórico limitado (ex.: 24h) no free tier | baixo |
| D. Só avisar | Banner persistente "acima do limite da licença" sem bloquear | mínimo |

Recomendação preliminar: **B + D** — não descartar dado (ruim para avaliação do
produto), mas deixar o limite visível e a visão analítica reduzida.

## Aceite (após decisão)

- Comportamento escolhido implementado e coberto por teste
- Tela de Licença descreve exatamente o que o free tier limita
