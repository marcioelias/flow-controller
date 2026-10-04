# Task 17.13 — Templates e amostragem sobrevivem ao reinício do coletor

**Phase:** 17 (Drill-down por IP)
**Files:** `template-cache/src/lib.rs`, `netflow-parser/src/lib.rs`, `metrics/src/lib.rs`,
`collector-core/src/main.rs`, `collector-core/src/template_store.rs` (NEW)

## Problema

Relato do Marcio (04/10/2026): o IPv6 "demora a aparecer". Na homologação, depois do
reinício do coletor (cada deploy), os templates dos dois domínios do BNG só chegaram 8
e 14 minutos depois. Até lá:

- conjuntos de dados sem template conhecido são **descartados em silêncio**
  (`netflow-parser`, `templates.get(..) → None → vec![]`), sem métrica;
- a **taxa de amostragem** (1:1000) e a hora de boot do roteador (IE 160) também só
  voltam quando o options template e seus dados chegam — até lá os volumes ficam
  **1 000× menores**.

O roteador reenvia templates em intervalo próprio (no NetStream, minutos); IPv4 e IPv6
usam templates distintos, então uma família pode ficar sem dados mais tempo que a outra.

## Regras

- **R-01 — Persistir o estado de decodificação.** Templates (dados e options),
  taxa de amostragem e hora de boot por `(exportador, domínio)` são gravados em
  `TEMPLATE_STATE_PATH` (padrão `./templates-state.json`, no volume do coletor) quando
  **mudam** — reenvio idêntico não grava. Escrita atômica (arquivo temporário + rename).
- **R-02 — Carregar no boot.** Ao subir, todos os workers recebem o estado salvo antes de
  abrir o socket UDP; o primeiro pacote de dados já é decodificado e escalado.
  Entradas não vistas há mais de **24 h** são ignoradas.
- **R-03 — O roteador manda.** Template recebido sempre substitui o carregado (mesma
  chave), então uma definição que mudou com o coletor parado se corrige no primeiro
  reenvio.
- **R-04 — Métrica.** `data_sets_without_template_total`: conjuntos de dados descartados
  por template desconhecido. Deve ficar em zero após o boot com estado salvo.
- **R-05 — Operação.** O operador ainda deve configurar o roteador para reenviar
  templates com frequência (1–2 min) — cobre reinício do roteador e perda de pacote.

## Aceite

- AC-01: reiniciar o coletor com templates salvos → dados IPv4 e IPv6 decodificados no
  primeiro pacote, já escalados pela amostragem.
- AC-02: reenvio idêntico do template não reescreve o arquivo.
- AC-03: arquivo ausente ou corrompido → coletor sobe normalmente (sem estado), com aviso.
- AC-04: estado com mais de 24 h não é carregado.
