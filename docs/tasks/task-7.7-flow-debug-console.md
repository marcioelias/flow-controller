# Task 7.7 — Flow Debug Console

**Status:** 🔲 A implementar  
**Fase:** 7 — Licensing, Health & Ops  
**Prioridade:** antes da Fase 8 (BGP)

---

## Objetivo

Console de depuração em tempo real para visualizar flows individuais conforme chegam
ao coletor, com filtro por IP de origem e painel de detalhe ao clicar em uma linha.
Destinado ao operador que precisa confirmar que um exporter está enviando flows corretamente.

---

## Arquitetura

### Backend — novo canal WebSocket `/ws/debug`

O WebSocket existente (`/ws`) transmite `LiveFlowStats` (bytes agregados por device).
O debug console precisa de flows individuais **antes** da agregação.

O worker de parsing já tem os flows decodificados em `parse_packet(...)`. Antes de
passá-los ao agregador, uma cópia é enviada para um broadcast channel separado
`debug_tx: broadcast::Sender<DebugFlow>`. O endpoint `/ws/debug` assina esse canal.

**Capacidade do canal:** 2048 entradas — flows em excesso são descartados silenciosamente
(o debug console não é garantido, não pode afetar o path crítico).

**Filtragem por IP:** feita no servidor, após o subscribe, para não transmitir
flows desnecessários. O cliente passa `?src_ip=10.0.0.5` ao conectar; se omitido,
transmite todos (limitado a 100 flows/s por conexão para não saturar).

```rust
pub struct DebugFlow {
    pub timestamp_sec: u32,
    pub exporter_ip:   String,
    pub src_ip:        String,
    pub dst_ip:        String,
    pub src_port:      u16,
    pub dst_port:      u16,
    pub protocol:      u8,
    pub bytes:         u64,
    pub packets:       u64,
    pub src_asn:       u32,
    pub dst_asn:       u32,
    pub ingress_if:    u32,
    pub egress_if:     u32,
    pub tcp_flags:     u8,
    pub flow_count:    u64,
}
```

### Worker thread → canal debug

No `worker_loop`, após `parse_packet`, antes do agregador:

```rust
for flow in &parsed_flows {
    let _ = debug_tx.try_send(DebugFlow::from(flow));  // try_send = não bloqueia
}
```

`debug_tx` é passado ao worker via `Arc` (clone do sender).

### WebSocket handler `/ws/debug`

```
GET /ws/debug?src_ip=10.0.0.5   (filtro opcional)
GET /ws/debug                    (todos os flows)
```

- Pública como `/ws` (mesma decisão — melhorar auth em task futura)
- Assina o `broadcast::Receiver<DebugFlow>`
- Filtra por `src_ip` se fornecido
- Rate limit: envia no máximo 1 mensagem a cada 10ms por conexão (evita flood no browser)
- Fecha silenciosamente ao acumular lag > 512 mensagens (cliente lento)

---

## Frontend — `DebugConsole.vue`

### Layout

Modal de tela cheia (não bloqueia, mas ocupa 90% da viewport) aberta pelo botão
**"Debug"** na topbar, ao lado do badge "Live".

```
┌─────────────────────────────────────────────────────────────────┐
│ Flow Debug Console          Filtro: [src_ip input]  [●] Live  ✕ │
├───────────────────────────────┬─────────────────────────────────┤
│ LISTA DE FLOWS (rolagem auto) │ DETALHE DO FLOW SELECIONADO     │
│                               │                                 │
│ 10:42:31 10.0.0.5→8.8.8.8   │  Timestamp   2026-07-08 10:42:31│
│ ▶ 10:42:31 10.0.0.3→1.1.1.1 │  Exporter    10.10.0.1          │
│ 10:42:32 10.0.0.5→8.8.8.8   │                                 │
│ 10:42:32 10.0.0.7→9.9.9.9   │  Origem      10.0.0.3           │
│ 10:42:32 10.0.0.3→1.1.1.1   │  Src Port    52341              │
│ ...                           │  Src ASN     65001              │
│                               │                                 │
│                               │  Destino     1.1.1.1           │
│                               │  Dst Port    443  (HTTPS)      │
│                               │  Dst ASN     13335 (Cloudflare)│
│                               │                                 │
│                               │  Protocolo   TCP (6)           │
│                               │  Bytes       1.4 KB            │
│                               │  Pacotes     3                 │
│                               │  TCP Flags   SYN ACK           │
│                               │                                 │
│                               │  If. Entrada  Gi0/0/1  (ifIdx 3)│
│                               │  If. Saída    Gi0/0/2  (ifIdx 4)│
│                               │  Flow Count  1                 │
│                               │                                 │
│                               │  ┌──────────────────────────┐  │
│                               │  │ 4 flows novos deste IP   │  │
│                               │  │ [Ver mais recente]       │  │
│                               │  └──────────────────────────┘  │
├───────────────────────────────┴─────────────────────────────────┤
│ 1.247 flows recebidos · 0 perdidos · conectado há 00:02:14      │
└─────────────────────────────────────────────────────────────────┘
```

### Comportamento da lista

- Máximo de **500 linhas** em memória — remove as mais antigas automaticamente
- **Scroll automático** para o fim quando nenhum flow está selecionado
- Ao selecionar um flow, scroll automático **pausa** (o usuário está lendo)
- Linha selecionada marcada com `bg-emerald-500/10 border-l-2 border-emerald-500`
- Colunas: hora | src_ip → dst_ip:port | protocolo | tamanho
- Protocolo exibido como nome: TCP, UDP, ICMP, GRE, ESP, etc.

### Comportamento do painel de detalhe

- Aparece ao clicar em qualquer linha
- **Não atualiza automaticamente** enquanto há seleção — o flow clicado fica fixo
- Badge "N flows novos deste IP desde seleção" incrementa conforme chegam
- Botão "Ver mais recente" seleciona o flow mais recente do mesmo `src_ip`
- Botão "✕" no detalhe fecha e retoma scroll automático

### Filtro por IP

- Input de texto no header da modal
- Ao digitar, reconecta o WebSocket com `?src_ip=<valor>`
- Debounce de 500ms antes de reconectar
- Limpa a lista ao reconectar

### Stats do rodapé

- Flows recebidos nesta sessão (contador)
- Flows perdidos (mensagens WebSocket com gap de sequência — opcional, best-effort)
- Tempo de conexão (timer)

### Botão de abertura — topbar

```html
<!-- AppLayout.vue — ao lado do badge "Live" -->
<button @click="debugOpen = true"
  class="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-zinc-800 border border-zinc-700
         text-xs font-medium text-zinc-400 hover:text-zinc-200 hover:border-zinc-500 transition-all">
  <BugPlay class="w-3.5 h-3.5" />
  Debug
</button>
```

Ícone: `BugPlay` (lucide-vue-next).

---

## TCP Flags — decodificação

```typescript
function decodeTcpFlags(flags: number): string {
  const names = ['FIN','SYN','RST','PSH','ACK','URG','ECE','CWR']
  return names.filter((_, i) => flags & (1 << i)).join(' ') || '—'
}
```

## Protocolo — nome

```typescript
const PROTO_NAMES: Record<number, string> = {
  1: 'ICMP', 6: 'TCP', 17: 'UDP', 47: 'GRE',
  50: 'ESP', 51: 'AH', 89: 'OSPF', 132: 'SCTP'
}
function protoName(n: number) { return PROTO_NAMES[n] ?? `Proto ${n}` }
```

---

## Arquivos a criar/modificar

| Arquivo | Ação |
|---------|------|
| `collector-core/src/main.rs` | `debug_tx` broadcast channel, passa para workers, handler `/ws/debug` |
| `collector-core/src/main.rs` | `DebugFlow` struct + `Serialize` |
| `frontend/src/views/DebugConsole.vue` | Nova view (modal) |
| `frontend/src/layouts/AppLayout.vue` | Botão "Debug" na topbar + `<DebugConsole>` montado |

Não precisa de store Pinia — estado local da modal (`ref`) é suficiente.

---

## Aceite

- [ ] `/ws/debug` transmite flows individuais em tempo real
- [ ] `?src_ip=` filtra corretamente no servidor
- [ ] Lista limita a 500 linhas, remove as mais antigas
- [ ] Scroll automático para ao selecionar um flow
- [ ] Detalhe exibe todos os campos de `DebugFlow` com rótulos legíveis
- [ ] Badge "N flows novos" incrementa com flows do mesmo src_ip após seleção
- [ ] Botão "Ver mais recente" salta para o flow mais novo do IP
- [ ] Reconexão automática ao fechar/reabrir a modal
- [ ] `try_send` no worker — canal cheio descarta silenciosamente, sem bloquear o path crítico
- [ ] Compilação sem warnings
