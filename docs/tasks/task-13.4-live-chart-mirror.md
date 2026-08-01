# Task 13.4 — Gráfico ao vivo espelhado e sem suavização

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 1h
**Files:** `collector-core/src/main.rs`, `frontend/src/views/Dashboard.vue`

## Problema

1. O gráfico "Tráfego em tempo real" do Dashboard só sobe — `LiveFlowStats` carrega
   apenas `total_bytes`, a direção (IE 61) se perde no broadcast. O Histórico já é
   espelhado (entrada pra cima, saída pra baixo); o ao vivo precisa do mesmo.
2. A linha é suavizada (curva). Para tráfego de rede isso é ruim: esconde microburst,
   arredonda pico e sugere transições que não existem. Ferramenta séria de NOC usa
   interpolação linear ou step.

## Implementação

### 1. Backend — direção no payload do WS

```rust
pub struct LiveFlowStats {
    pub timestamp_sec: u32,
    pub total_bytes: u64,
    pub bytes_in: u64,   // direction = ingress
    pub bytes_out: u64,  // direction = egress
    pub per_device: HashMap<String, u64>,
}
```

Flows com `direction = 255` (não reportado): somados em `bytes_in` por convenção
(maioria dos deployments de borda enxerga download como dominante), e contabilizados
em `total_bytes` sempre. Quando o fallback por prefixo (Fase 11) classificar, ele já
terá reescrito `direction` antes do broadcast.

### 2. Frontend — espelho e linha dura

- Série de entrada plotada positiva, saída negativa (mesma convenção do Histórico)
- Interpolação linear (sem tension/smoothing); step opcional se ficar mais legível
- Eixo Y simétrico com labels absolutos (sem "-" nos negativos)

## Aceite

- Dashboard ao vivo com entrada acima do eixo e saída abaixo
- Nenhuma curva de suavização em gráfico de tráfego (Dashboard e Histórico)
- Total continua batendo com a soma in+out
