# Task 7.1 — Software Licensing System

## Status: IMPLEMENTADO

---

## Visão Geral

Sistema de licenciamento offline baseado em criptografia assimétrica Ed25519.

- A **chave privada** fica exclusivamente com o desenvolvedor — nunca entra no binário distribuído.
- A **chave pública** é embarcada em tempo de compilação no `collector-core`.
- Uma licença é matematicamente impossível de forjar sem a chave privada.
- A validação é 100% offline — o collector não precisa de internet para validar.

---

## Limites do tier free (sem licença)

| Recurso | Limite |
|---------|--------|
| Banda máxima monitorada | 100 Mbps |
| Top Talkers exibidos | 1 |

Quando nenhum arquivo de licença é encontrado, o collector opera em tier free automaticamente.

---

## Arquivos implementados

| Arquivo | Papel |
|---------|-------|
| `license-gen/src/main.rs` | Ferramenta do desenvolvedor (não distribuída) |
| `collector-core/src/license.rs` | Módulo de validação embedded no collector |
| `collector-core/src/auth.rs` | `AppState` com campo `license` |
| `collector-core/src/main.rs` | Startup + endpoints REST `/api/license` |
| `frontend/src/views/License.vue` | Tela de visualização/atualização de licença |
| `frontend/src/router.ts` | Rota `/license` |
| `frontend/src/layouts/AppLayout.vue` | Link "Licença" na sidebar (admin) |

---

## Formato da Licença

```
<base64url(json_payload)>.<base64url(assinatura_ed25519)>
```

A assinatura é calculada sobre os **bytes UTF-8 exatos** do JSON (a parte antes do ponto).

### Payload JSON

```json
{
  "fingerprint": "a3f8c2d1e4b5f6a7",
  "licensee":    "ISP Exemplo Ltda",
  "max_bps":     1000000000,
  "max_talkers": 10,
  "issued_at":   "2025-07-07",
  "expires_at":  "2027-01-01"
}
```

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `fingerprint` | string | SHA-256 do machine-id + MAC + DMI UUID, primeiros 32 hex chars |
| `licensee` | string | Nome do cliente |
| `max_bps` | number \| null | Banda máxima em bytes/sec. `null` = ilimitado |
| `max_talkers` | number \| null | Top Talkers máximos. `null` = ilimitado |
| `issued_at` | string | Data de emissão (YYYY-MM-DD) |
| `expires_at` | string \| null | Data de expiração. `null` = perpétua |

### Tiers comuns

| Tier | `max_bps` | `max_talkers` |
|------|-----------|---------------|
| Free (default) | 100_000_000 | 1 |
| 1 Gbps | 1_000_000_000 | 10 |
| 10 Gbps | 10_000_000_000 | 50 |
| 100 Gbps | 100_000_000_000 | 200 |
| Ilimitado | null | null |

---

## Machine Fingerprint

Gerado por `license::machine_fingerprint()`:

```
SHA-256( /etc/machine-id + ":" + MAC_primaria + ":" + /sys/class/dmi/id/product_uuid )
→ primeiros 32 caracteres hex
```

Fontes usadas (cada uma ignorada silenciosamente se indisponível):
- `/etc/machine-id` — muda com reinstalação do OS
- MAC da primeira interface não-loopback em `/sys/class/net/*/address`
- `/sys/class/dmi/id/product_uuid` — UUID da placa-mãe (pode não existir em containers)

Qualquer alteração em uma das fontes muda o fingerprint e invalida a licença.

---

## Fluxo de Emissão de Licença (passo a passo)

### Passo 1 — Setup único (desenvolvedor)

```bash
# Gera o par de chaves. Executa UMA vez.
# Salva privkey em ~/.config/flow-license/private.key (chmod 600)
# Imprime a pubkey base64 no stdout
cargo run --bin license-gen -- keygen
```

Saída:
```
Keypair generated and saved to /home/user/.config/flow-license/private.key

Public key (embed this in collector-core/src/license.rs):
  Base64: <PUBKEY_BASE64>
```

### Passo 2 — Substituir a chave pública no collector

Editar `collector-core/src/license.rs` linha com `PUBLIC_KEY_B64`:

```rust
const PUBLIC_KEY_B64: &str = "<PUBKEY_BASE64_DO_PASSO_1>";
```

Recompilar e redistribuir o binário.

### Passo 3 — Cliente obtém o fingerprint

Na interface web do collector, menu **Administração → Licença**:
- O fingerprint é exibido e pode ser copiado com um clique.

Ou via API (sem auth):
```bash
curl http://<collector>:8080/api/license | jq .fingerprint
```

### Passo 4 — Desenvolvedor gera a licença

```bash
cargo run --bin license-gen -- issue \
  --fingerprint a3f8c2d1e4b5f6a7b8c9d0e1f2a3b4c5 \
  --licensee "ISP Exemplo Ltda" \
  --max-bps 1000000000 \
  --max-talkers 10 \
  --expires 2027-01-01
```

Saída (string a ser entregue ao cliente):
```
eyJmaW5nZXJwcmludCI6ImEzZjh....<assinatura>
```

Para licença perpétua, omitir `--expires`.
Para banda ilimitada, omitir `--max-bps`.

### Passo 5 — Cliente aplica a licença

Na interface web, menu **Licença**, campo "Apply License":
1. Cole a string recebida
2. Clique "Aplicar Licença"
3. O collector valida, salva em `/etc/flow-collector/license.key` (fallback `./license.key`) e exibe o novo status

Ou via API:
```bash
curl -X POST http://<collector>:8080/api/license \
  -H "Content-Type: application/json" \
  -d '{"license": "eyJmaW5nZXJwcmludC..."}'
```

---

## API REST

Ambos os endpoints são **públicos** (sem autenticação) para que a tela de licença seja
acessível antes mesmo do primeiro login.

### GET /api/license

Retorna o status atual da licença.

```json
{
  "valid": true,
  "licensee": "ISP Exemplo Ltda",
  "tier_label": "1 Gbps / 10 talkers",
  "max_bps": 1000000000,
  "max_talkers": 10,
  "expires_at": "2027-01-01",
  "fingerprint": "a3f8c2d1e4b5f6a7",
  "message": null
}
```

Sem arquivo de licença:
```json
{
  "valid": true,
  "licensee": null,
  "tier_label": "Free (100 Mbps / 1 talker)",
  "max_bps": 100000000,
  "max_talkers": 1,
  "expires_at": null,
  "fingerprint": "a3f8c2d1e4b5f6a7",
  "message": null
}
```

### POST /api/license

Aplica uma nova licença.

**Request:**
```json
{ "license": "eyJmaW5nZXJwcmludCI6ImEzZjh..." }
```

**Response 200:** novo `LicenseStatus` (mesmo formato do GET)

**Response 400:**
```json
{ "error": "fingerprint mismatch: expected a3f8... got b9c1..." }
{ "error": "license expired on 2024-01-01" }
{ "error": "invalid signature" }
```

---

## Verificação de licença (license-gen verify)

Para confirmar que uma licença gerada é válida antes de enviar ao cliente:

```bash
cargo run --bin license-gen -- verify eyJmaW5nZXJwcmludCI6ImEzZjh...

# Saída:
Signature: VALID
Payload:
  fingerprint:  a3f8c2d1e4b5f6a7b8c9d0e1f2a3b4c5
  licensee:     ISP Exemplo Ltda
  max_bps:      1000000000 (1 Gbps)
  max_talkers:  10
  issued_at:    2025-07-07
  expires_at:   2027-01-01
```

---

## Segurança

| Ameaça | Mitigação |
|--------|-----------|
| Compartilhamento de licença entre servidores | Fingerprint vincula licença a hardware específico |
| Forjamento de licença | Ed25519 — impossível sem a chave privada |
| Patch do binário para pular checagem | Possível por engenharia reversa, mas fora do modelo de ameaça para ISPs |
| Perda da chave privada | Fazer backup de `~/.config/flow-license/private.key` — se perdida, impossível emitir novas licenças para o mesmo binário. Uma nova keypair exige recompilação e redistribuição. |

---

## Backup da chave privada

```bash
# Backup
cp ~/.config/flow-license/private.key /seu/backup/seguro/

# Restaurar em nova máquina
mkdir -p ~/.config/flow-license
cp /seu/backup/private.key ~/.config/flow-license/private.key
chmod 600 ~/.config/flow-license/private.key
```
