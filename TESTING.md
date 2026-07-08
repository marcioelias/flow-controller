# Guia de Testes

Este documento explica como executar os testes do projeto.

## Testes Backend (Rust)

### Executar todos os testes

```bash
cargo test
```

### Executar testes de um módulo específico

```bash
cargo test --lib -p collector-core
```

### Executar testes com output detalhado

```bash
cargo test -- --nocapture
```

### Cobertura de testes (requer cargo-tarpaulin)

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

## Testes Frontend (Vue + TypeScript)

### Instalar dependências (primeira vez)

```bash
cd frontend
npm install
```

### Executar testes em modo watch

```bash
npm test
```

### Executar testes uma vez (CI)

```bash
npm run test:run
```

### Executar testes com interface visual

```bash
npm run test:ui
```

## Testes Implementados

### Backend
- ✅ `auth::hash_password` - Verifica se senha é hasheada corretamente
- ✅ `auth::verify_password` - Verifica autenticação de senha
- ✅ `auth::generate_token` - Verifica geração de JWT
- ✅ `auth::validate_token` - Verifica validação de JWT

### Frontend
- ✅ Auth Store - Teste completo de autenticação
  - Login/logout
  - Persistência no localStorage
  - Headers de autenticação
  - Identificação de admin

## Rodando Testes no Docker

### Backend (durante o build)

```bash
docker build --target test -t flow-collector-test .
docker run flow-collector-test cargo test
```

## Integração Contínua

Os testes devem ser executados antes de fazer merge:

```bash
# Backend
cargo test

# Frontend
cd frontend && npm run test:run
```

## Adicionando Novos Testes

### Backend (Rust)

Adicione testes no final do arquivo do módulo:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_function() {
        assert_eq!(my_function(), expected_value);
    }
}
```

### Frontend (Vue/TypeScript)

Crie arquivo `*.test.ts` ao lado do arquivo sendo testado:

```typescript
import { describe, it, expect } from 'vitest'

describe('MyComponent', () => {
  it('should work', () => {
    expect(true).toBe(true)
  })
})
```

## Debugging Testes

### Backend
```bash
cargo test -- --nocapture test_name
```

### Frontend
```bash
npm run test:ui  # Abre interface visual com debugging
```
