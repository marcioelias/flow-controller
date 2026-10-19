# Task 17.16 — ML ignora mensagens de erro ICMP

**Phase:** 17 (Drill-down por IP)
**Files:** `collector-core/src/features.rs`

## Problema

Em 19/10/2026 todos os alertas ML de IPs de CGNAT na homologação vinham de ICMP *Time
Exceeded* (tipo 11) para `170.231.6.0/7.0` — tráfego gerado pela rede (loop, task
17.15), não pelo assinante. O `100.68.1.5` estava offline desde 16/10 segundo o RADIUS
e sem nenhum outro tráfego, mas seguia "anômalo" com 1,85 Mpps de upload.

## Regras

- **R-01** — As features do ML por IP não contam mensagens de **erro** ICMP:
  ICMPv4 tipos 3 (destino inalcançável), 4, 5, 11 (*Time Exceeded*) e 12; ICMPv6 tipos
  1, 2, 3 e 4. No NetFlow o tipo vem no byte alto de `dst_port` (`tipo << 8 | código`).
- **R-02** — Echo (ping, tipos 0/8 e 128/129) continua contando: é comportamento do host.
- **R-03** — Nada muda na gravação nem nos painéis — só o que alimenta o ML.

## Aceite

- AC-01: IP cujo único tráfego é ICMP tipo 11 não gera vetor de features.
- AC-02: IP com ping e HTTPS mantém ambos nas features.
