use std::env;

use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::models::{ModelProfileInput, ModelProfileSummary, ModelProfileUpdate};

#[derive(Clone, Debug, FromRow)]
struct ModelProfileRow {
    id: String,
    company_id: String,
    name: String,
    protocol: String,
    base_url: String,
    encrypted_api_key: Option<String>,
    api_key_hint: Option<String>,
    model: String,
    enabled: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Debug)]
pub struct ModelProfileCredentials {
    pub protocol: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub enabled: bool,
}

pub async fn list(pool: &SqlitePool, company_id: &str) -> Result<Vec<ModelProfileSummary>> {
    let rows = sqlx::query_as::<_, ModelProfileRow>(
        "SELECT id, company_id, name, protocol, base_url, encrypted_api_key,
                api_key_hint, model, enabled, created_at, updated_at
         FROM model_profiles WHERE company_id = ? ORDER BY created_at, id",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(summary).collect())
}

pub async fn get_credentials(
    pool: &SqlitePool,
    profile_id: &str,
) -> Result<ModelProfileCredentials> {
    let row = sqlx::query_as::<_, ModelProfileRow>(
        "SELECT id, company_id, name, protocol, base_url, encrypted_api_key,
                api_key_hint, model, enabled, created_at, updated_at
         FROM model_profiles WHERE id = ?",
    )
    .bind(profile_id)
    .fetch_one(pool)
    .await?;
    let api_key = row
        .encrypted_api_key
        .as_deref()
        .map(decrypt_api_key)
        .transpose()?;
    Ok(ModelProfileCredentials {
        protocol: row.protocol,
        base_url: row.base_url,
        api_key,
        model: row.model,
        enabled: row.enabled != 0,
    })
}

pub async fn create(
    pool: &SqlitePool,
    company_id: &str,
    input: ModelProfileInput,
) -> Result<ModelProfileSummary> {
    validate_input(&input)?;
    let now = Utc::now().to_rfc3339();
    let id = format!("model-profile-{}", Uuid::new_v4());
    let (encrypted_key, hint) = encrypt_optional(input.api_key.as_deref())?;
    sqlx::query(
        "INSERT INTO model_profiles
         (id, company_id, name, protocol, base_url, encrypted_api_key, api_key_hint,
          model, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(company_id)
    .bind(input.name.trim())
    .bind(input.protocol.trim().to_lowercase())
    .bind(input.base_url.trim().trim_end_matches('/'))
    .bind(encrypted_key)
    .bind(hint)
    .bind(input.model.trim())
    .bind(input.enabled)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    get_summary(pool, &id).await
}

pub async fn update(
    pool: &SqlitePool,
    profile_id: &str,
    input: ModelProfileUpdate,
) -> Result<ModelProfileSummary> {
    let existing = sqlx::query_as::<_, ModelProfileRow>(
        "SELECT id, company_id, name, protocol, base_url, encrypted_api_key,
                api_key_hint, model, enabled, created_at, updated_at
         FROM model_profiles WHERE id = ?",
    )
    .bind(profile_id)
    .fetch_one(pool)
    .await?;
    let next_protocol = input
        .protocol
        .unwrap_or(existing.protocol.clone())
        .trim()
        .to_lowercase();
    let next_base_url = input
        .base_url
        .unwrap_or(existing.base_url.clone())
        .trim()
        .trim_end_matches('/')
        .to_string();
    let next_model = input.model.unwrap_or(existing.model.clone());
    if next_protocol != "mock" && (next_base_url.is_empty() || next_model.trim().is_empty()) {
        return Err(anyhow!(
            "OpenAI Compatible Provider requires base_url and model"
        ));
    }
    let (encrypted_key, hint) = match input.api_key.as_deref() {
        Some(value) if !value.trim().is_empty() => {
            let (encrypted, hint) = encrypt_optional(Some(value))?;
            (encrypted, hint)
        }
        Some(_) => (None, None),
        None => (existing.encrypted_api_key, existing.api_key_hint),
    };
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE model_profiles SET name = ?, protocol = ?, base_url = ?,
         encrypted_api_key = ?, api_key_hint = ?, model = ?, enabled = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(input.name.unwrap_or(existing.name))
    .bind(next_protocol)
    .bind(next_base_url)
    .bind(encrypted_key)
    .bind(hint)
    .bind(next_model)
    .bind(input.enabled.unwrap_or(existing.enabled != 0))
    .bind(now)
    .bind(profile_id)
    .execute(pool)
    .await?;
    get_summary(pool, profile_id).await
}

pub async fn get_summary(pool: &SqlitePool, profile_id: &str) -> Result<ModelProfileSummary> {
    let row = sqlx::query_as::<_, ModelProfileRow>(
        "SELECT id, company_id, name, protocol, base_url, encrypted_api_key,
                api_key_hint, model, enabled, created_at, updated_at
         FROM model_profiles WHERE id = ?",
    )
    .bind(profile_id)
    .fetch_one(pool)
    .await?;
    Ok(summary(row))
}

fn summary(row: ModelProfileRow) -> ModelProfileSummary {
    let _ = (&row.company_id, &row.created_at, &row.updated_at);
    ModelProfileSummary {
        id: row.id,
        name: row.name,
        protocol: row.protocol,
        base_url: row.base_url,
        api_key_configured: row.encrypted_api_key.is_some(),
        api_key_hint: row.api_key_hint,
        model: row.model,
        enabled: row.enabled != 0,
    }
}

fn validate_input(input: &ModelProfileInput) -> Result<()> {
    if input.name.trim().is_empty() || input.model.trim().is_empty() {
        return Err(anyhow!("Provider name and model are required"));
    }
    if input.protocol.trim().eq_ignore_ascii_case("mock") {
        return Ok(());
    }
    if !input
        .protocol
        .trim()
        .eq_ignore_ascii_case("openai_compatible")
        && !input
            .protocol
            .trim()
            .eq_ignore_ascii_case("openai-compatible")
    {
        return Err(anyhow!(
            "Only mock and openai_compatible protocols are supported"
        ));
    }
    if input.base_url.trim().is_empty() {
        return Err(anyhow!("OpenAI Compatible Provider requires base_url"));
    }
    Ok(())
}

fn encrypt_optional(value: Option<&str>) -> Result<(Option<String>, Option<String>)> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok((None, None));
    };
    let key = encryption_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| anyhow!("invalid encryption key"))?;
    let nonce_bytes: [u8; 12] = Uuid::new_v4().as_bytes()[..12]
        .try_into()
        .expect("UUID has at least 12 bytes");
    let nonce = Nonce::from_slice(&nonce_bytes);
    let mut encrypted = nonce_bytes.to_vec();
    encrypted.extend(
        cipher
            .encrypt(nonce, value.as_bytes())
            .map_err(|_| anyhow!("encrypt API key"))?,
    );
    let hint = value
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    Ok((
        Some(STANDARD.encode(encrypted)),
        Some(format!("••••{hint}")),
    ))
}

fn decrypt_api_key(value: &str) -> Result<String> {
    let bytes = STANDARD.decode(value).context("decode encrypted API key")?;
    if bytes.len() <= 12 {
        return Err(anyhow!("encrypted API key is invalid"));
    }
    let key = encryption_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| anyhow!("invalid encryption key"))?;
    let nonce = Nonce::from_slice(&bytes[..12]);
    Ok(String::from_utf8(
        cipher
            .decrypt(nonce, &bytes[12..])
            .map_err(|_| anyhow!("decrypt API key"))?,
    )?)
}

fn encryption_key() -> Result<[u8; 32]> {
    let secret = env::var("MODEL_PROFILE_ENCRYPTION_KEY")
        .context("MODEL_PROFILE_ENCRYPTION_KEY is required to save or use API keys")?;
    if secret.trim().is_empty() {
        return Err(anyhow!("MODEL_PROFILE_ENCRYPTION_KEY cannot be empty"));
    }
    Ok(Sha256::digest(secret.as_bytes()).into())
}

#[cfg(test)]
mod tests {
    use super::{decrypt_api_key, encrypt_optional};

    #[test]
    fn api_key_round_trip_never_stores_plaintext() {
        std::env::set_var(
            "MODEL_PROFILE_ENCRYPTION_KEY",
            "unit-test-encryption-secret",
        );
        let (encrypted, hint) = encrypt_optional(Some("sk-test-secret-1234")).expect("encrypt");
        let encrypted = encrypted.expect("encrypted value");
        assert!(!encrypted.contains("sk-test-secret-1234"));
        assert_eq!(hint.as_deref(), Some("••••1234"));
        assert_eq!(
            decrypt_api_key(&encrypted).expect("decrypt"),
            "sk-test-secret-1234"
        );
    }
}
