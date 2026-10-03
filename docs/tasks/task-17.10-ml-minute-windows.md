# Task 17.10 — ML: janelas de 1 minuto, baseline de 24 h, persistência e limiar por percentil

**Phase:** 17 (Drill-down por IP)
**Files:** `collector-core/src/ml_runner.rs`, `collector-core/src/ml_model.rs`,
`collector-core/src/settings.rs`, `docs/tasks/task-15.5-ml-significance-floor.md` (nota)

## Problema

Relato do Marcio (03/10/2026): em produção o ML "pega muita coisa" e a maioria é falso
positivo. Medido na homologação (BNG, NetStream **1:1000**):

- ~1 160 IPs de origem por segundo; 8 322 IPs ativos em 3 min; **52 alertas ML/hora**
  num único exportador.
- **Piso inócuo.** Com 1:1000, um pacote sorteado num segundo vira 1 000 pps — 100% dos
  IPs passam do piso de 100 pps / 1 Mbps (task 15.5).
- **Janela de 1 s é ruído.** Por segundo um IP tem 0, 1 ou 2 pacotes amostrados: as
  features são degraus e o Isolation Forest isola combinações que são só acaso.
- **Baseline de ~40 s.** Buffer de 50 000 amostras a ~1 160 IPs/s, retreinado a cada
  360 amostras (< 1 s): o modelo só "lembra" o último minuto — sem perfil diário.
- **Taxa de base.** ~4 milhões de pontuações/hora com limiar fixo 0,65: uma cauda
  minúscula já são dezenas de alertas.
- **Sem persistência.** Um único segundo anômalo dispara o alerta.

## Regras

- **R-01 — Janela de 1 minuto.** As features por segundo são acumuladas por
  `(exportador, IP, minuto)`. Volumes somam; `unique_dst_ips` / `unique_dst_ports` usam o
  **máximo por segundo** dentro do minuto (os conjuntos não chegam ao runner). Taxas
  (pps, bps) são médias do minuto. Um minuto fecha quando o relógio passa do seu fim
  + 90 s (flows chegam atrasados até o active timeout); dados de minuto já fechado são
  descartados.
- **R-02 — Piso em amostras reais.** Um minuto só é pontuado se tiver pelo menos
  `ML_MIN_SAMPLES` pacotes **amostrados** (pacotes ÷ taxa de amostragem do exportador;
  padrão 10). Os pisos `ML_MIN_PPS` / `ML_MIN_BPS` continuam valendo sobre as médias do
  minuto.
- **R-03 — Baseline de 24 h.** Por exportador, 24 reservatórios horários (até 2 000
  vetores cada, amostragem de reservatório dentro da hora). O treino usa a união e
  acontece a cada 10 min. Pronto quando houver ≥ 5 000 vetores **e** ≥ 60 min de dados.
- **R-04 — Limiar por percentil.** Após cada treino, o limiar do exportador é o
  percentil `100 − ML_ALERT_PERCENTILE` das pontuações do próprio conjunto de treino
  (padrão `ML_ALERT_PERCENTILE = 0.1` → topo 0,1%), nunca abaixo de 0,6. O ajuste por
  falso positivo da task 15.2 (+0,05 por FP, até +0,20) soma sobre esse limiar.
- **R-05 — Persistência.** Um IP só gera alerta se estiver acima do limiar em **3 dos
  últimos 5 minutos** pontuados. Cooldown por IP sobe de 5 para **15 min**.
- **R-06 — Severidade.** `critical` se a pontuação do minuto atual ≥ limiar + 0,1 **e**
  o IP foi anômalo nos 5 minutos; senão `warning`.
- **R-07 — Mensagem.** Inclui o limiar efetivo e a persistência, ex.:
  `ML anomaly: score=0.71 (threshold=0.64, p99.9) 4/5 min pps=… ul_ratio=…`.

## Configurações (grupo IA)

| Chave | Padrão | Uso |
|---|---|---|
| `ML_MIN_SAMPLES` | 10 | pacotes amostrados mínimos no minuto para pontuar |
| `ML_ALERT_PERCENTILE` | 0.1 | % superior das pontuações que pode alertar |

## Aceite

- AC-01: minuto com 3 pacotes amostrados (1:1000 → 3 000 pacotes) não é pontuado.
- AC-02: IP anômalo em 1 minuto isolado não alerta; em 3 de 5, alerta uma vez e só
  volta a alertar após 15 min.
- AC-03: limiar = percentil configurado das pontuações de treino, com piso 0,6.
- AC-04: reservatório horário mantém no máximo 2 000 vetores por hora e 24 horas.
- AC-05: na homologação, alertas ML/hora caem de ~52 para poucos por hora.
