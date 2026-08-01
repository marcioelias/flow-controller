# Task 13.1 — Decodificar flowStart / flowEnd

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 1h
**Files:** `netflow-parser/src/lib.rs`, `flow-types/src/lib.rs`

## Problema

O protocolo informa quando o flow começou e terminou — o parser ignora e o coletor
carimba tudo com o horário de chegada. Um upload de 134 MiB numa conexão única de 20s
vira um espigão num único segundo, datado do momento em que o exporter resolveu
exportar (active timeout), não do momento em que o tráfego aconteceu.

## Implementação

### 1. Novos campos em `NormalizedFlow`

```rust
/// Início/fim reais do flow em unix ms (0 = exporter não informou)
pub start_ms: u64,
pub end_ms: u64,
```

### 2. IEs decodificados

| ID | Nome | Formato |
|----|------|---------|
| 21 | LAST_SWITCHED (v9) | ms relativo ao sysUptime do header |
| 22 | FIRST_SWITCHED (v9) | ms relativo ao sysUptime do header |
| 150 | flowStartSeconds | unix s |
| 151 | flowEndSeconds | unix s |
| 152 | flowStartMilliseconds | unix ms |
| 153 | flowEndMilliseconds | unix ms |
| 154/155 | flowStart/EndMicroseconds | unix µs → /1000 |

### 3. Conversão v9 (sysUptime-relativo, wrap-safe)

```rust
// age em ms desde que o evento ocorreu, robusto a wrap de 32 bits (~49,7 dias)
let age_ms = header.sys_uptime.wrapping_sub(first_switched);
let start_ms = (header.unix_secs as u64 * 1000).saturating_sub(age_ms as u64);
```

`parse_data_set` passa a receber o `sys_uptime` do header v9 (None para IPFIX).

## Aceite

- v9 com FIRST/LAST_SWITCHED → `start_ms`/`end_ms` corretos, inclusive com uptime
  próximo do wrap
- IPFIX com IE 150/151 e 152/153
- Exporter sem esses IEs → campos ficam 0 (comportamento atual preservado)
- Testes unitários cobrindo v9 relativo, IPFIX absoluto e wrap
