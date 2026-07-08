/// Software licensing module for the Flow Collector.
///
/// The Ed25519 public key is embedded at compile time as `PUBLIC_KEY_B64`.
/// After running `license-gen keygen`, copy the printed "raw base64url no-pad" value here.
///
/// License file is looked up in this order:
///   1. /etc/flow-collector/license.key
///   2. ./license.key
///
/// If no license file is found the collector runs in the free tier (100 Mbps / 1 talker).

// ---------------------------------------------------------------------------
// Embedded public key — replace with output of `license-gen keygen`
// (the "raw 32 bytes, base64url no-pad" line)
// ---------------------------------------------------------------------------
const PUBLIC_KEY_B64: &str = "oG37QWu5ktfk0OrvfaQHh87wjAQY56p1b04DLJT2kUg";

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    pub fingerprint: String,
    pub licensee: String,
    pub max_bps: Option<u64>,
    pub max_talkers: Option<u32>,
    pub issued_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LicenseStatus {
    pub valid: bool,
    pub licensee: Option<String>,
    pub tier_label: String,
    pub max_bps: Option<u64>,
    pub max_talkers: Option<u32>,
    pub expires_at: Option<String>,
    /// The current machine's hardware fingerprint (always populated).
    pub fingerprint: String,
    /// Non-None when `valid == false` — describes why the license is invalid.
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------
// Free tier constants
// ---------------------------------------------------------------------------

const FREE_MAX_BPS: u64 = 100_000_000; // 100 Mbps
const FREE_MAX_TALKERS: u32 = 1;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns the default free-tier `LicenseStatus`.
pub fn free_tier() -> LicenseStatus {
    LicenseStatus {
        valid: true,
        licensee: None,
        tier_label: "Free (100 Mbps / 1 talker)".to_string(),
        max_bps: Some(FREE_MAX_BPS),
        max_talkers: Some(FREE_MAX_TALKERS),
        expires_at: None,
        fingerprint: machine_fingerprint(),
        message: None,
    }
}

/// Derives a stable 32-hex-char fingerprint for this machine.
///
/// Sources (each silently skipped if unavailable):
/// - `/etc/machine-id`
/// - Primary (non-loopback) MAC address from `/sys/class/net/*/address`
/// - `/sys/class/dmi/id/product_uuid`
///
/// The SHA-256 digest of all available values joined with `\n` is computed
/// and the first 32 hex characters are returned. The result is deterministic
/// for the same hardware/OS state.
pub fn machine_fingerprint() -> String {
    let mut parts: Vec<String> = Vec::new();

    // /etc/machine-id
    if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
        let trimmed = id.trim().to_string();
        if !trimmed.is_empty() {
            parts.push(trimmed);
        }
    }

    // Primary (non-loopback) MAC address
    if let Some(mac) = first_mac_address() {
        parts.push(mac);
    }

    // DMI product UUID (may be absent in containers / VMs that hide it)
    if let Ok(uuid) = std::fs::read_to_string("/sys/class/dmi/id/product_uuid") {
        let trimmed = uuid.trim().to_string();
        if !trimmed.is_empty() {
            parts.push(trimmed);
        }
    }

    let combined = parts.join("\n");
    let digest = Sha256::digest(combined.as_bytes());
    hex::encode(&digest[..16]) // 16 bytes → 32 hex chars
}

/// Loads and validates a license from the standard paths.
///
/// Returns `free_tier()` when no license file exists.
/// Returns a `LicenseStatus` with `valid = false` on any validation error.
pub fn load_and_validate(primary_path: &str) -> LicenseStatus {
    let path = find_license_file(primary_path);

    let license_str = match path {
        None => return free_tier(),
        Some(ref p) => match std::fs::read_to_string(p) {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                return invalid_status(
                    format!("cannot read license file '{}': {}", p, e),
                    machine_fingerprint(),
                )
            }
        },
    };

    validate_license_string(&license_str)
}

/// Validates a license string (two base64url parts separated by `.`).
/// Used both by `load_and_validate` and the POST /api/license endpoint.
pub fn validate_license_string(license_str: &str) -> LicenseStatus {
    let fp = machine_fingerprint();

    // --- Decode public key ---
    let pubkey_bytes = match URL_SAFE_NO_PAD.decode(PUBLIC_KEY_B64) {
        Ok(b) => b,
        Err(_) => {
            // Placeholder hasn't been replaced yet — log and return free tier
            // so the collector still starts during development.
            tracing::warn!(
                "PUBLIC_KEY_B64 is a placeholder; running in free tier. \
                 Run `license-gen keygen` and embed the result."
            );
            return free_tier();
        }
    };

    let pubkey_arr: [u8; 32] = match pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return invalid_status("embedded public key is not 32 bytes".to_string(), fp)
        }
    };

    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&pubkey_arr) {
        Ok(k) => k,
        Err(e) => return invalid_status(format!("invalid public key: {}", e), fp),
    };

    // --- Split license string ---
    let dot = match license_str.find('.') {
        Some(i) => i,
        None => return invalid_status("license has no '.' separator".to_string(), fp),
    };
    let payload_b64 = &license_str[..dot];
    let sig_b64 = &license_str[dot + 1..];

    // --- Decode parts ---
    let json_bytes = match URL_SAFE_NO_PAD.decode(payload_b64) {
        Ok(b) => b,
        Err(e) => return invalid_status(format!("base64 decode error (payload): {}", e), fp),
    };

    let sig_bytes = match URL_SAFE_NO_PAD.decode(sig_b64) {
        Ok(b) => b,
        Err(e) => return invalid_status(format!("base64 decode error (signature): {}", e), fp),
    };

    let sig_arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return invalid_status("signature must be 64 bytes".to_string(), fp),
    };

    let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);

    // --- Verify signature ---
    use ed25519_dalek::Verifier;
    if let Err(e) = verifying_key.verify(&json_bytes, &signature) {
        return invalid_status(format!("invalid signature: {}", e), fp);
    }

    // --- Parse payload ---
    let payload: LicensePayload = match serde_json::from_slice(&json_bytes) {
        Ok(p) => p,
        Err(e) => return invalid_status(format!("JSON parse error: {}", e), fp),
    };

    // --- Check fingerprint ---
    if payload.fingerprint != fp {
        return invalid_status(
            format!(
                "fingerprint mismatch: license is for '{}', this machine is '{}'",
                payload.fingerprint, fp
            ),
            fp,
        );
    }

    // --- Check expiry ---
    if let Some(ref exp) = payload.expires_at {
        match chrono::NaiveDate::parse_from_str(exp, "%Y-%m-%d") {
            Ok(expiry_date) => {
                let today = chrono::Utc::now().date_naive();
                if today > expiry_date {
                    return invalid_status(
                        format!("license expired on {}", exp),
                        fp,
                    );
                }
            }
            Err(e) => {
                return invalid_status(format!("invalid expires_at date '{}': {}", exp, e), fp)
            }
        }
    }

    // --- All checks passed ---
    let tier_label = build_tier_label(payload.max_bps, payload.max_talkers);

    LicenseStatus {
        valid: true,
        licensee: Some(payload.licensee),
        tier_label,
        max_bps: payload.max_bps,
        max_talkers: payload.max_talkers,
        expires_at: payload.expires_at,
        fingerprint: fp,
        message: None,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn find_license_file(primary: &str) -> Option<String> {
    for path in &[primary, "./license.key"] {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}

fn invalid_status(message: String, fingerprint: String) -> LicenseStatus {
    // Fall back to free-tier limits so the collector remains usable
    LicenseStatus {
        valid: false,
        licensee: None,
        tier_label: "Free (100 Mbps / 1 talker)".to_string(),
        max_bps: Some(FREE_MAX_BPS),
        max_talkers: Some(FREE_MAX_TALKERS),
        expires_at: None,
        fingerprint,
        message: Some(message),
    }
}

fn build_tier_label(max_bps: Option<u64>, max_talkers: Option<u32>) -> String {
    match (max_bps, max_talkers) {
        (None, None) => "Unlimited".to_string(),
        (bps, talkers) => {
            let bps_str = match bps {
                None => "unlimited".to_string(),
                Some(b) => humanize_bps(b),
            };
            let talkers_str = match talkers {
                None => "unlimited talkers".to_string(),
                Some(1) => "1 talker".to_string(),
                Some(n) => format!("{} talkers", n),
            };
            format!("{} / {}", bps_str, talkers_str)
        }
    }
}

fn humanize_bps(bps: u64) -> String {
    const GBPS: u64 = 1_000_000_000;
    const MBPS: u64 = 1_000_000;
    const KBPS: u64 = 1_000;

    if bps >= GBPS && bps % GBPS == 0 {
        format!("{} Gbps", bps / GBPS)
    } else if bps >= GBPS {
        format!("{:.2} Gbps", bps as f64 / GBPS as f64)
    } else if bps >= MBPS && bps % MBPS == 0 {
        format!("{} Mbps", bps / MBPS)
    } else if bps >= MBPS {
        format!("{:.2} Mbps", bps as f64 / MBPS as f64)
    } else if bps >= KBPS {
        format!("{} Kbps", bps / KBPS)
    } else {
        format!("{} bps", bps)
    }
}

fn first_mac_address() -> Option<String> {
    let net_dir = std::path::Path::new("/sys/class/net");
    let entries = std::fs::read_dir(net_dir).ok()?;

    let mut macs: Vec<String> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let iface = entry.file_name();
            let iface_str = iface.to_string_lossy();

            // Skip loopback
            if iface_str == "lo" {
                return None;
            }

            let addr_path = net_dir.join(iface_str.as_ref()).join("address");
            let mac = std::fs::read_to_string(addr_path).ok()?;
            let mac = mac.trim().to_string();

            // Skip all-zero and broadcast MACs
            if mac == "00:00:00:00:00:00" || mac.is_empty() {
                return None;
            }

            Some(mac)
        })
        .collect();

    // Sort for determinism across runs
    macs.sort();
    macs.into_iter().next()
}
