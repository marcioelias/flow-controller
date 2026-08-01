# Task 13.8 — Timestamps IPFIX relativos (IE 21/22 + IE 160)

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 1.5h
**Files:** `netflow-parser/src/lib.rs`, `template-cache/src/lib.rs`

## Problema (encontrado em lab com softflowd)

A task 13.1 assumiu que IPFIX usa timestamps absolutos (IE 150–155). Mas exporters
derivados de NetFlow — softflowd incluído — exportam **IE 21/22
(flowEnd/StartSysUpTime, ms relativos ao boot)** também em IPFIX. Ao contrário do
v9, o header IPFIX não tem sysUptime, então a conversão ficava sem base:
`start_ms = 0` → sem spreading → o flow inteiro no segundo de chegada → espigões de
~1,4 Gbps num link de 100 Mbps.

## Implementação

### 1. Aprender a base de tempo via options (IE 160)

Exporters que usam IE 21/22 em IPFIX anunciam `systemInitTimeMilliseconds` (IE 160,
unix ms do boot) num options data record. Cache por `(exporter_ip, domain)` no
`ThreadLocalTemplateCache`, como o sampling:

```
start_ms = sys_init_ms + flowStartSysUpTime
```

### 2. Fallback por duração quando IE 160 não veio

`last − first` é a **duração exata** do flow, independente da base. Flows são
exportados logo após expirar, então:

```
end_ms   = export_ms
start_ms = export_ms − (last − first)   // wrapping_sub para o wrap de 49,7 dias
```

Aproximação: desloca o intervalo alguns segundos para frente (o tempo entre expirar
e exportar), mas preserva a duração — o formato da rampa fica correto.

### 3. Resolução movida para depois do loop de campos

IE 21/22 precisam um do outro e do contexto — saem do `decode_field` e são
resolvidos por registro (`resolve_switched_times`), nos caminhos fixo e varlen.

## Aceite

- softflowd IPFIX: flows espalhados pela duração real (sem espigão de arrival)
- v9: comportamento inalterado
- Testes: IPFIX 21/22 com IE 160; IPFIX 21/22 sem IE 160 (fallback); wrap
