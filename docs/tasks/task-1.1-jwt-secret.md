# Task 1.1 — Secure JWT Secret via Environment Variable

**Phase:** 1 (Critical Fixes)  
**Effort:** 15 minutes  
**Files:** `collector-core/src/auth.rs`

## Problem

JWT secret is hardcoded:
```rust
// auth.rs line ~80
const JWT_SECRET: &[u8] = b"your-secret-key-change-this-in-production";
```

Any attacker who reads the binary or source can forge tokens.

## Implementation

1. Replace the constant with a `OnceLock<String>` initialized from `JWT_SECRET` env var.
2. Fall back to the hardcoded value only if the env var is missing, but emit a `tracing::warn!` on startup.

```rust
use std::sync::OnceLock;

static JWT_SECRET: OnceLock<String> = OnceLock::new();

fn get_jwt_secret() -> &'static [u8] {
    JWT_SECRET.get_or_init(|| {
        match std::env::var("JWT_SECRET") {
            Ok(s) => s,
            Err(_) => {
                tracing::warn!("JWT_SECRET env var not set; using insecure default");
                "your-secret-key-change-this-in-production".to_string()
            }
        }
    }).as_bytes()
}
```

3. Replace every use of `JWT_SECRET` with `get_jwt_secret()` — there are two call sites:
   - `generate_token` (encoding)
   - `validate_token` (decoding)

4. Add `JWT_SECRET` to `docker-compose.yml` environment section for `flow-collector`:
```yaml
environment:
  - CLICKHOUSE_URL=http://clickhouse:8123
  - JWT_SECRET=change-this-to-a-random-64-char-string
```

## Acceptance Criteria

- `cargo build` succeeds
- Starting without `JWT_SECRET` env var prints a warning but still starts
- Setting `JWT_SECRET=newsecret` in env causes `generate_token` to use the new value
- Existing tokens signed with the old secret become invalid after env var change (expected)
