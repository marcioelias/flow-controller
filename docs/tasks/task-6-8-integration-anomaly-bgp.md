# Task 6.8 — Integração: Anomaly Detector → BGP Blackhole Automático

**Depende de:** task-6.2 (anomaly detector), task-8.3 (route control bridge)  
**Esforço:** ~2h  
**Arquivos:** `collector-core/src/anomaly_detector.rs`, `collector-core/src/bgp_control.rs`

---

## Objetivo

Quando o anomaly detector disparar um alerta de `attack_signature`, criar automaticamente
um anúncio BGP de blackhole para o `src_ip` atacante e registrar o evento no `alert_events`
com `bgp_announced = 1`.

O fluxo é opt-in por regra: cada `alert_rule` pode ter `auto_bgp_announce: bool` nos seus
`params` JSON.

---

## Fluxo de dados

```
[anomaly_detector background task]
  └─ detecta src_ip com attack_signature
  └─ cria alert_event (bgp_announced = 0)
  └─ se rule.params.auto_bgp_announce == true:
       └─ chama bgp_control::announce_prefix(pool, src_ip + "/32", next_hop="self", community_id=None)
       └─ atualiza alert_event: bgp_announced = 1, origin_detail = "alert_event#<id>"
       └─ registra bgp_announcements: origin = "anomaly_detector", origin_detail = "alert_event#<id>"
```

---

## Parâmetros de regra adicionais (Fase 6.8)

Adicionar ao JSON `params` de `alert_rules` com `rule_type = 'attack_signature'`:

```json
{
  "auto_bgp_announce": false,
  "bgp_withdraw_after_min": 60,
  "bgp_community_id": null
}
```

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `auto_bgp_announce` | bool | Se true, anuncia /32 via BGP ao disparar alerta |
| `bgp_withdraw_after_min` | u32 | Retirar automaticamente após N minutos (0 = nunca) |
| `bgp_community_id` | u32? | ID de `bgp_communities` a aplicar (null = sem community) |

---

## Implementação

### `anomaly_detector.rs` — chamar control bridge

```rust
if rule_params.auto_bgp_announce {
    match bgp_control::announce_prefix(
        &state.db,
        &state.exabgp_pipe,
        &format!("{}/32", src_ip),
        "self",
        rule_params.bgp_community_id,
        Some(format!("alert_event#{}", event_id)),
        "anomaly_detector",
    ).await {
        Ok(ann_id) => {
            // schedule withdraw if bgp_withdraw_after_min > 0
            if rule_params.bgp_withdraw_after_min > 0 {
                schedule_bgp_withdraw(&state, ann_id, rule_params.bgp_withdraw_after_min);
            }
            sqlx::query("UPDATE alert_events SET bgp_announced = 1 WHERE id = ?")
                .bind(event_id).execute(&state.db).await.ok();
        }
        Err(e) => tracing::error!("auto BGP announce failed for {}: {}", src_ip, e),
    }
}
```

### Withdraw agendado

Usar `tokio::time::sleep` + `bgp_control::withdraw_prefix(pool, pipe, ann_id)` em uma
task separada para não bloquear o detector.

---

## UI (task-8.6 extension)

Na tela de alert rules, exibir os campos `auto_bgp_announce`, `bgp_withdraw_after_min`,
`bgp_community_id` como campos editáveis quando `rule_type == 'attack_signature'`.

Na tabela de `alert_events`, exibir badge "BGP ativo" ou "BGP retirado" com base em
`bgp_announced` + `bgp_announcements.withdrawn_at`.

---

## Acceptance criteria

- [ ] Alerta com `auto_bgp_announce: true` gera entrada em `bgp_announcements` com `origin = "anomaly_detector"`
- [ ] `alert_events.bgp_announced` é marcado como 1 após anúncio bem-sucedido
- [ ] Withdraw automático ocorre após `bgp_withdraw_after_min` minutos
- [ ] Falha no FIFO (ExaBGP offline) não cancela criação do alert_event
- [ ] `cargo build` passa
