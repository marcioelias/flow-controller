use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::sync::{Arc, OnceLock};

const TOKEN_EXPIRATION_HOURS: i64 = 24;

static JWT_SECRET_STORE: OnceLock<String> = OnceLock::new();

fn get_jwt_secret() -> &'static [u8] {
    JWT_SECRET_STORE
        .get_or_init(|| match std::env::var("JWT_SECRET") {
            Ok(s) => s,
            Err(_) => {
                tracing::warn!("JWT_SECRET env var not set; using insecure default");
                "your-secret-key-change-this-in-production".to_string()
            }
        })
        .as_bytes()
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_admin: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // username
    pub exp: usize,  // expiration time
    pub is_admin: bool,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub is_admin: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub username: String,
    pub password: Option<String>,
    pub is_admin: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub ws_tx: tokio::sync::broadcast::Sender<crate::LiveFlowStats>,
    pub debug_tx: tokio::sync::broadcast::Sender<crate::DebugFlow>,
    pub metrics: std::sync::Arc<metrics::CollectorMetrics>,
    pub license: std::sync::Arc<std::sync::RwLock<crate::license::LicenseStatus>>,
    pub clickhouse_url: String,
    pub bgp_sessions: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, crate::bgp::BgpSessionState>>>,
    pub exabgp_pipe: String,
    pub exabgp_config_path: String,
    pub ml_status: crate::ml_runner::SharedMlStatus,
}

/// Initialize SQLite database and create default admin user
pub async fn init_db() -> anyhow::Result<SqlitePool> {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename("./auth.db")
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(options).await?;

    // Create users table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            is_admin BOOLEAN NOT NULL DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Check if admin user exists
    let admin_exists: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM users WHERE username = 'admin'")
            .fetch_optional(&pool)
            .await?;

    if admin_exists.is_none() {
        // Create default admin user (admin / admin123)
        let password_hash = bcrypt::hash("admin123", bcrypt::DEFAULT_COST)?;
        sqlx::query(
            "INSERT INTO users (username, password_hash, is_admin) VALUES (?, ?, ?)",
        )
        .bind("admin")
        .bind(password_hash)
        .bind(true)
        .execute(&pool)
        .await?;

        tracing::info!("Default admin user created (username: admin, password: admin123)");
    }

    Ok(pool)
}

/// Hash a password using bcrypt
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    Ok(bcrypt::hash(password, bcrypt::DEFAULT_COST)?)
}

/// Verify a password against a hash
pub fn verify_password(password: &str, hash: &str) -> anyhow::Result<bool> {
    Ok(bcrypt::verify(password, hash)?)
}

/// Generate a JWT token for a user
pub fn generate_token(username: &str, is_admin: bool) -> anyhow::Result<String> {
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(TOKEN_EXPIRATION_HOURS))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: username.to_string(),
        exp: expiration,
        is_admin,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(get_jwt_secret()),
    )?;

    Ok(token)
}

/// Validate a JWT token and extract claims
pub fn validate_token(token: &str) -> anyhow::Result<Claims> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(get_jwt_secret()),
        &Validation::default(),
    )?;

    Ok(token_data.claims)
}

/// Login handler
pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    // Fetch user from database
    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(&payload.username)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user = user.ok_or(StatusCode::UNAUTHORIZED)?;

    // Verify password
    if !verify_password(&payload.password, &user.password_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Generate token
    let token = generate_token(&user.username, user.is_admin)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LoginResponse {
        token,
        user: UserResponse {
            id: user.id,
            username: user.username,
            is_admin: user.is_admin,
        },
    }))
}

/// List all users (admin only)
pub async fn list_users_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<UserResponse>>, StatusCode> {
    let users: Vec<User> = sqlx::query_as("SELECT * FROM users")
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = users
        .into_iter()
        .map(|u| UserResponse {
            id: u.id,
            username: u.username,
            is_admin: u.is_admin,
        })
        .collect();

    Ok(Json(response))
}

/// Create a new user (admin only)
pub async fn create_user_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, StatusCode> {
    // Hash the password
    let password_hash = hash_password(&payload.password)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Insert user into database
    let result = sqlx::query(
        "INSERT INTO users (username, password_hash, is_admin) VALUES (?, ?, ?)",
    )
    .bind(&payload.username)
    .bind(&password_hash)
    .bind(payload.is_admin)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create user: {}", e);
        StatusCode::CONFLICT // Username already exists
    })?;

    Ok(Json(UserResponse {
        id: result.last_insert_rowid(),
        username: payload.username,
        is_admin: payload.is_admin,
    }))
}

/// Update an existing user (admin only)
pub async fn update_user_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(user_id): axum::extract::Path<i64>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, StatusCode> {
    if let Some(password) = payload.password {
        if !password.is_empty() {
            let password_hash = hash_password(&password)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            sqlx::query(
                "UPDATE users SET username = ?, password_hash = ?, is_admin = ? WHERE id = ?",
            )
            .bind(&payload.username)
            .bind(&password_hash)
            .bind(payload.is_admin)
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update user: {}", e);
                StatusCode::CONFLICT
            })?;
        } else {
            sqlx::query("UPDATE users SET username = ?, is_admin = ? WHERE id = ?")
                .bind(&payload.username)
                .bind(payload.is_admin)
                .bind(user_id)
                .execute(&state.db)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to update user: {}", e);
                    StatusCode::CONFLICT
                })?;
        }
    } else {
        sqlx::query("UPDATE users SET username = ?, is_admin = ? WHERE id = ?")
            .bind(&payload.username)
            .bind(payload.is_admin)
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update user: {}", e);
                StatusCode::CONFLICT
            })?;
    }

    Ok(Json(UserResponse {
        id: user_id,
        username: payload.username,
        is_admin: payload.is_admin,
    }))
}


/// Delete a user (admin only)
pub async fn delete_user_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(user_id): axum::extract::Path<i64>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password() {
        let password = "test123";
        let hash = hash_password(password).expect("should hash password");

        // Hash should be different from original
        assert_ne!(password, hash);

        // Hash should start with bcrypt prefix
        assert!(hash.starts_with("$2"));
    }

    #[test]
    fn test_verify_password_correct() {
        let password = "test123";
        let hash = hash_password(password).expect("should hash password");

        let result = verify_password(password, &hash).expect("should verify");
        assert!(result, "correct password should verify");
    }

    #[test]
    fn test_verify_password_incorrect() {
        let password = "test123";
        let hash = hash_password(password).expect("should hash password");

        let result = verify_password("wrong", &hash).expect("should verify");
        assert!(!result, "incorrect password should not verify");
    }

    #[test]
    fn test_generate_token() {
        let username = "testuser";
        let is_admin = true;

        let token = generate_token(username, is_admin).expect("should generate token");

        // Token should not be empty
        assert!(!token.is_empty());

        // Token should have 3 parts separated by dots (JWT format)
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn test_validate_token() {
        let username = "testuser";
        let is_admin = true;

        let token = generate_token(username, is_admin).expect("should generate token");
        let claims = validate_token(&token).expect("should validate token");

        assert_eq!(claims.sub, username);
        assert_eq!(claims.is_admin, is_admin);
    }

    #[test]
    fn test_validate_invalid_token() {
        let result = validate_token("invalid.token.here");
        assert!(result.is_err(), "invalid token should fail validation");
    }
}
