# Task 17.15 — Detector de pingue-pongue (loop de roteamento)

**Phase:** 17 (Drill-down por IP)
**Depende de:** 17.14 (interfaces e próximo salto gravados)
**Files:** `collector-core/src/detector.rs`, `collector-core/src/alerts.rs`,
`collector-core/src/settings.rs`, `frontend/src/views/AlertEvents.vue`

## Problema

Em 19/10/2026 o BNG de Imbituba exportava, havia pelo menos 7 dias, ICMP *Time
Exceeded* (tipo 11) de IPs de CGNAT para `170.231.6.0` e `170.231.7.0` entrando e saindo
pela **mesma interface** rumo ao mesmo próximo salto — assinatura de pacotes indo e
voltando entre dois roteadores até o TTL zerar. O volume aparente (dezenas de TiB/dia,
~45% do total do BNG) distorcia todos os painéis e nada alertou sobre isso.

## Regras

- **R-01 — Sinal.** Por exportador e por **destino**, a cada minuto: pacotes amostrados
  com `in_if = out_if` (entra e sai pela mesma interface) e, separadamente, pacotes ICMP
  tipo 11 (*Time Exceeded*; ICMPv6 tipo 3).
- **R-02 — Disparo.** Alerta `routing_loop` quando um destino tiver, por **5 minutos
  seguidos**, pelo menos `LOOP_MIN_SAMPLES` pacotes amostrados/minuto (padrão 100)
  entrando e saindo pela mesma interface **ou** recebendo ICMP *Time Exceeded*.
  Cooldown de 1 h por (exportador, destino).
- **R-03 — Conteúdo.** O alerta traz destino, interface(s), próximo salto, as origens que
  mais aparecem, pps estimado e quanto do total do exportador esse tráfego representa.
  Severidade `critical` se passar de 10% do total do exportador.
- **R-04** — Roda como as demais regras do detector, por exportador, e pode ser
  desligado em Configurações (`LOOP_DETECTOR_ENABLED`, padrão ligado).

## Aceite

- AC-01: destino com 150 amostras/min em `in_if = out_if` por 5 min → 1 alerta;
  continua em loop → nenhum novo alerta antes de 1 h.
- AC-02: tráfego normal (entra numa interface e sai por outra) não dispara.
- AC-03: com os dados de 19/10 do BNG de Imbituba, `170.231.6.0` e `170.231.7.0` disparam.
