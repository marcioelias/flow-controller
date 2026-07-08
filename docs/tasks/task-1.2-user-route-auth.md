# Task 1.2 — Apply Auth Middleware to User Routes

**Phase:** 1 (Critical Fixes)  
**Effort:** 15 minutes  
**Files:** `collector-core/src/main.rs`, `collector-core/src/middleware.rs`

## Problem

User management routes (`GET /api/users`, `POST /api/users`, etc.) have no auth middleware applied.
Anyone who knows the URL can list, create, or delete users without a token.

From `middleware.rs`:
```rust
#[allow(dead_code)]  // ← this middleware exists but is never used
pub async fn require_auth(...) { ... }
```

## Current Route Setup (main.rs)

```rust
let user_routes = Router::new()
    .route("/users", get(list_users_handler).post(create_user_handler))
    .route("/users/:id", put(update_user_handler).delete(delete_user_handler));
    // no .layer(require_admin) here
```

## Implementation

1. Remove `#[allow(dead_code)]` from `require_auth` in `middleware.rs`.

2. In `main.rs`, apply `require_admin` layer to user routes (same as exporter routes):

```rust
let user_routes = Router::new()
    .route("/users", get(list_users_handler).post(create_user_handler))
    .route("/users/:id", put(update_user_handler).delete(delete_user_handler))
    .layer(axum::middleware::from_fn_with_state(
        state.clone(),
        middleware::require_admin,
    ));
```

Look at how `exporter_routes` is already protected in `main.rs` and replicate exactly.

## Acceptance Criteria

- `GET /api/users` without `Authorization` header returns `401`
- `GET /api/users` with a valid non-admin JWT returns `403`
- `GET /api/users` with a valid admin JWT returns `200` with user list
- `cargo build` succeeds
