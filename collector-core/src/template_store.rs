//! Decode state (templates, sampling rates, exporter boot times) persisted
//! across collector restarts (task 17.13), so the first data packet after a
//! restart is already decodable and correctly scaled.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use template_cache::{Template, TemplateField, TemplateKey, ThreadLocalTemplateCache};

/// Entries not seen for this long are not loaded (R-02)
const MAX_AGE_SECS: u64 = 24 * 3_600;

#[derive(Serialize, Deserialize, Clone)]
struct StoredTemplate {
    exporter_ip: Ipv4Addr,
    source_id: u32,
    template_id: u16,
    is_options: bool,
    scope_field_count: u16,
    /// (field_type, length)
    fields: Vec<(u16, u16)>,
    seen_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct DecodeState {
    saved_at: u64,
    templates: Vec<StoredTemplate>,
    /// (exporter, domain, rate)
    sampling: Vec<(Ipv4Addr, u32, u32)>,
    /// (exporter, domain, boot time in unix ms)
    sys_inits: Vec<(Ipv4Addr, u32, u64)>,
}

pub fn state_path() -> PathBuf {
    std::env::var("TEMPLATE_STATE_PATH")
        .unwrap_or_else(|_| "./templates-state.json".to_string())
        .into()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Snapshot of one worker's cache
pub fn snapshot(cache: &ThreadLocalTemplateCache) -> DecodeState {
    DecodeState {
        saved_at: unix_now(),
        templates: cache
            .templates()
            .map(|t| StoredTemplate {
                exporter_ip: t.key.exporter_ip,
                source_id: t.key.source_id,
                template_id: t.key.template_id,
                is_options: t.is_options,
                scope_field_count: t.scope_field_count,
                fields: t.fields.iter().map(|f| (f.field_type, f.length)).collect(),
                seen_at: t.timestamp,
            })
            .collect(),
        sampling: cache
            .sampling_rates()
            .map(|((ip, d), r)| (*ip, *d, *r))
            .collect(),
        sys_inits: cache
            .sys_inits()
            .map(|((ip, d), ms)| (*ip, *d, *ms))
            .collect(),
    }
}

/// Union of every worker's snapshot; the newest copy of a template wins
fn merge(parts: &HashMap<usize, DecodeState>) -> DecodeState {
    let mut templates: HashMap<(Ipv4Addr, u32, u16), StoredTemplate> = HashMap::new();
    let mut sampling: HashMap<(Ipv4Addr, u32), u32> = HashMap::new();
    let mut sys_inits: HashMap<(Ipv4Addr, u32), u64> = HashMap::new();
    let mut saved_at = 0;
    for part in parts.values() {
        saved_at = saved_at.max(part.saved_at);
        for t in &part.templates {
            let k = (t.exporter_ip, t.source_id, t.template_id);
            if templates.get(&k).is_none_or(|old| old.seen_at <= t.seen_at) {
                templates.insert(k, t.clone());
            }
        }
        for (ip, d, r) in &part.sampling {
            sampling.insert((*ip, *d), *r);
        }
        for (ip, d, ms) in &part.sys_inits {
            sys_inits.insert((*ip, *d), *ms);
        }
    }
    DecodeState {
        saved_at,
        templates: templates.into_values().collect(),
        sampling: sampling
            .into_iter()
            .map(|((ip, d), r)| (ip, d, r))
            .collect(),
        sys_inits: sys_inits
            .into_iter()
            .map(|((ip, d), ms)| (ip, d, ms))
            .collect(),
    }
}

fn write_atomic(path: &Path, state: &DecodeState) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(state)?)?;
    std::fs::rename(tmp, path)
}

/// Reads the saved state, dropping entries older than 24 h (R-02). Missing or
/// corrupt file → `None` with a warning (AC-03).
pub fn load(path: &Path, now: u64) -> Option<DecodeState> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!("Template state {}: {e}", path.display());
            return None;
        }
    };
    let mut state: DecodeState = match serde_json::from_slice(&bytes) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Template state {} ignored (corrupt): {e}", path.display());
            return None;
        }
    };
    state
        .templates
        .retain(|t| now.saturating_sub(t.seen_at) < MAX_AGE_SECS);
    if now.saturating_sub(state.saved_at) >= MAX_AGE_SECS {
        state.sampling.clear();
        state.sys_inits.clear();
    }
    Some(state)
}

/// Seeds a worker's cache with the saved state
pub fn apply(state: &DecodeState, cache: &mut ThreadLocalTemplateCache) {
    let templates = state
        .templates
        .iter()
        .map(|t| Template {
            key: TemplateKey {
                exporter_ip: t.exporter_ip,
                source_id: t.source_id,
                template_id: t.template_id,
            },
            fields: t
                .fields
                .iter()
                .map(|(field_type, length)| TemplateField {
                    field_type: *field_type,
                    length: *length,
                })
                .collect(),
            is_options: t.is_options,
            scope_field_count: t.scope_field_count,
            timestamp: t.seen_at,
        })
        .collect();
    cache.restore(
        templates,
        state
            .sampling
            .iter()
            .map(|(ip, d, r)| ((*ip, *d), *r))
            .collect(),
        state
            .sys_inits
            .iter()
            .map(|(ip, d, ms)| ((*ip, *d), *ms))
            .collect(),
    );
}

pub fn summary(state: &DecodeState) -> String {
    format!(
        "{} templates, {} sampling rates",
        state.templates.len(),
        state.sampling.len()
    )
}

/// Collects worker snapshots and writes the merged file. Runs on its own
/// thread: workers are plain threads and the write is tiny.
pub fn spawn_writer(
    path: PathBuf,
    initial: Option<DecodeState>,
) -> flume::Sender<(usize, DecodeState)> {
    let (tx, rx) = flume::unbounded::<(usize, DecodeState)>();
    std::thread::Builder::new()
        .name("template-store".into())
        .spawn(move || {
            let mut parts: HashMap<usize, DecodeState> = HashMap::new();
            // Keep what was loaded until the workers report their own state
            if let Some(s) = initial {
                parts.insert(usize::MAX, s);
            }
            while let Ok((worker, state)) = rx.recv() {
                parts.insert(worker, state);
                if let Err(e) = write_atomic(&path, &merge(&parts)) {
                    tracing::warn!("Template state write {} failed: {e}", path.display());
                }
            }
        })
        .expect("spawn template-store thread");
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_with_template(len: u16) -> ThreadLocalTemplateCache {
        let mut c = ThreadLocalTemplateCache::new();
        c.insert(Template {
            key: TemplateKey {
                exporter_ip: Ipv4Addr::new(172, 18, 0, 17),
                source_id: 2149548288,
                template_id: 1290,
            },
            fields: vec![TemplateField {
                field_type: 27,
                length: len,
            }],
            is_options: false,
            scope_field_count: 0,
            timestamp: 0,
        });
        c.set_sampling_rate(Ipv4Addr::new(172, 18, 0, 17), 2149548288, 1000);
        c
    }

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fv-template-store-{}-{name}.json",
            std::process::id()
        ))
    }

    // AC-01 (task 17.13)
    #[test]
    fn roundtrip_restores_templates_and_sampling() {
        let path = tmp_path("roundtrip");
        let mut parts = HashMap::new();
        parts.insert(0, snapshot(&cache_with_template(16)));
        write_atomic(&path, &merge(&parts)).unwrap();

        let state = load(&path, unix_now()).expect("state");
        let mut fresh = ThreadLocalTemplateCache::new();
        apply(&state, &mut fresh);
        let key = TemplateKey {
            exporter_ip: Ipv4Addr::new(172, 18, 0, 17),
            source_id: 2149548288,
            template_id: 1290,
        };
        assert_eq!(fresh.get(&key).unwrap().fields[0].length, 16);
        assert_eq!(
            fresh.sampling_rate(Ipv4Addr::new(172, 18, 0, 17), 2149548288),
            1000
        );
        std::fs::remove_file(path).ok();
    }

    // AC-03 (task 17.13)
    #[test]
    fn missing_or_corrupt_file_is_ignored() {
        assert!(load(&tmp_path("missing"), unix_now()).is_none());
        let path = tmp_path("corrupt");
        std::fs::write(&path, b"{not json").unwrap();
        assert!(load(&path, unix_now()).is_none());
        std::fs::remove_file(path).ok();
    }

    // AC-04 (task 17.13)
    #[test]
    fn stale_state_is_not_loaded() {
        let mut state = snapshot(&cache_with_template(16));
        let later = unix_now() + MAX_AGE_SECS + 10;
        let path = tmp_path("stale");
        state.saved_at = unix_now();
        write_atomic(&path, &state).unwrap();
        let loaded = load(&path, later).expect("state");
        assert!(loaded.templates.is_empty());
        assert!(loaded.sampling.is_empty());
        std::fs::remove_file(path).ok();
    }
}
