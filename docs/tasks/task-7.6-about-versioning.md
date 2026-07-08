# Task 7.6 — About & Versionamento de Componentes

**Status:** ✅ Implementado  
**Fase:** 7 — Licensing, Health & Ops  
**Versão introduzida:** 1.1

---

## Objetivo

Expor versões de todos os componentes do sistema na tela About, permitindo que o
suporte técnico identifique exatamente o ambiente em produção sem acesso ao servidor.

---

## Estratégia de versionamento

### Versão do produto
- Fonte única: arquivo `VERSION` na raiz do workspace (ex: `1.1`)
- Build number: `git rev-list --count HEAD` — incrementa automaticamente a cada commit
- Versão final: `major.minor.build` (ex: `1.1.47`)
- Backend: emitida via `build.rs` → `cargo:rustc-env=APP_VERSION`
- Frontend: lida em `vite.config.ts` → `define.__APP_VERSION__`

### Versões de dependências Rust
- `build.rs` parseia `Cargo.lock` em compile time
- Extrai versões das 22 libs principais (Axum, Tokio, SQLx, etc.)
- Emite `cargo:rustc-env=CARGO_DEPS=Label=ver,Label=ver,...`
- Retornado pelo `GET /api/version` como array `rust_deps`

### Versão do compilador Rust
- `build.rs` executa `rustc --version`
- Emite `cargo:rustc-env=RUSTC_VERSION`
- Retornado pelo `GET /api/version` como campo `rustc`

### Versão do ClickHouse
- Consultada em runtime pelo `GET /api/version` via `SELECT version()`
- Timeout de 2s; retorna `"unavailable"` se não responder

### Versões de dependências npm
- `vite.config.ts` parseia `package-lock.json` em build time
- Extrai versões resolvidas de 12 libs diretas
- Emite `define.__NPM_DEPS__` como array `[{name, version}]`
- Disponível no frontend sem round-trip

---

## Arquivos modificados / criados

| Arquivo | Ação |
|---------|------|
| `VERSION` | Alterado de `1.0.0` para `1.1` (major.minor apenas) |
| `collector-core/build.rs` | Adiciona `RUSTC_VERSION` e `CARGO_DEPS` |
| `collector-core/src/main.rs` | `version_handler` com State, `fetch_clickhouse_version` |
| `frontend/vite.config.ts` | Parseia `package-lock.json`, define `__NPM_DEPS__` |
| `frontend/src/vite-env.d.ts` | Declara `__NPM_DEPS__` |
| `frontend/src/views/About.vue` | Seção "Componentes" + seção "Dependências" com tabs Rust/npm |

---

## API

### GET /api/version (público)

```json
{
  "version":    "1.1.47",
  "name":       "FlowVision",
  "vendor":     "Hahn Tech Desenvolvimento e Consultoria Ltda",
  "rustc":      "rustc 1.82.0 (f6e511eec 2024-10-15)",
  "clickhouse": "24.3.2.23",
  "rust_deps": [
    { "name": "Axum",        "version": "0.7.9"   },
    { "name": "Tokio",       "version": "1.51.0"  },
    { "name": "Serde",       "version": "1.0.228" },
    { "name": "SQLx",        "version": "0.7.4"   },
    { "name": "clickhouse",  "version": "0.11.6"  }
  ]
}
```

---

## Tela About (`/about`, admin only)

Seções:
1. **Header** — logo FlowVision, badges `v{version}` e commit hash
2. **Licença** — dados de `GET /api/license` (licensee, tier, validade, fingerprint)
3. **Sobre o sistema** — descrição do produto e features
4. **Componentes** — grid 2×2: FlowVision, Commit, Rust compiler, ClickHouse (runtime)
5. **Dependências** — tabs Rust / npm, lista scrollável (max-h 14rem), nome + versão por linha
6. **Desenvolvedor** — Hahn Tech Desenvolvimento e Consultoria Ltda

---

## Aceite

- [x] `GET /api/version` retorna `rust_deps` com versões do `Cargo.lock`
- [x] `GET /api/version` retorna versão do ClickHouse consultada em runtime
- [x] Frontend exibe versão `1.1.X` com X automático por commit
- [x] Aba Rust mostra ≥ 20 dependências com versões corretas
- [x] Aba npm mostra dependências com versões do `package-lock.json`
- [x] About exibe dados de licença integrados
