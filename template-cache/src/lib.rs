use ahash::RandomState;
use std::collections::HashMap;
use std::net::Ipv4Addr;

/// The template key to identify unique Netflow v9 / IPFIX templates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateKey {
    pub exporter_ip: Ipv4Addr,
    pub source_id: u32, // Observation Domain ID in IPFIX
    pub template_id: u16,
}

/// Internal struct mapping Netflow fields to our NormalizedFlow
#[derive(Debug)]
pub struct TemplateField {
    pub field_type: u16,
    pub length: u16,
}

/// A parsed template representing a schema for a flow record
#[derive(Debug)]
pub struct Template {
    pub key: TemplateKey,
    pub fields: Vec<TemplateField>,
    /// Options templates (v9 flowset 1 / IPFIX set 3) describe metadata
    /// records (e.g. sampling rate), not flows
    pub is_options: bool,
    /// Number of scope fields at the start of `fields` (options templates only)
    pub scope_field_count: u16,
    /// Collector wall-clock time of the last (re)insert — used for expiration.
    /// Stamped locally on insert; the exporter's own clock is not trusted here
    /// because skew would make fresh templates look ancient.
    pub timestamp: u64,
}

/// A highly-performant cache designed to be owned by a single Worker thread.
/// Because Dispatcher hashes packets by exporter IP, worker threads don't need locks around this structure.
pub struct ThreadLocalTemplateCache {
    cache: HashMap<TemplateKey, Template, RandomState>,
    /// Sampling rate learned from options data records, per (exporter, source_id).
    /// 1 = unsampled / not learned yet.
    sampling_rates: HashMap<(Ipv4Addr, u32), u32, RandomState>,
    /// systemInitTimeMilliseconds (IE 160) por (exporter, domain) — base para
    /// converter flowStart/EndSysUpTime (IE 21/22) em unix ms no IPFIX
    sys_inits: HashMap<(Ipv4Addr, u32), u64, RandomState>,
}

impl Default for ThreadLocalTemplateCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadLocalTemplateCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::with_hasher(RandomState::new()),
            sampling_rates: HashMap::with_hasher(RandomState::new()),
            sys_inits: HashMap::with_hasher(RandomState::new()),
        }
    }

    /// Current sampling rate for an observation domain (1 = unsampled)
    pub fn sampling_rate(&self, exporter_ip: Ipv4Addr, source_id: u32) -> u32 {
        self.sampling_rates
            .get(&(exporter_ip, source_id))
            .copied()
            .unwrap_or(1)
            .max(1)
    }

    /// Record a sampling rate learned from an options data record.
    /// Logs when the rate is first learned or changes.
    pub fn set_sampling_rate(&mut self, exporter_ip: Ipv4Addr, source_id: u32, rate: u32) {
        let rate = rate.max(1);
        let prev = self.sampling_rates.insert((exporter_ip, source_id), rate);
        if prev != Some(rate) {
            tracing::info!(
                "Sampling rate for exporter {} (domain {}): 1:{}",
                exporter_ip,
                source_id,
                rate
            );
        }
    }

    /// Iterate learned sampling rates (for metrics export)
    pub fn sampling_rates(&self) -> impl Iterator<Item = (&(Ipv4Addr, u32), &u32)> {
        self.sampling_rates.iter()
    }

    pub fn sys_init_ms(&self, exporter_ip: Ipv4Addr, source_id: u32) -> Option<u64> {
        self.sys_inits.get(&(exporter_ip, source_id)).copied()
    }

    pub fn set_sys_init_ms(&mut self, exporter_ip: Ipv4Addr, source_id: u32, ms: u64) {
        self.sys_inits.insert((exporter_ip, source_id), ms);
    }

    pub fn get(&self, key: &TemplateKey) -> Option<&Template> {
        self.cache.get(key)
    }

    pub fn insert(&mut self, mut template: Template) {
        template.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.cache.insert(template.key, template);
    }

    pub fn prune_old_templates(&mut self, current_time: u64, max_age_secs: u64) {
        self.cache
            .retain(|_, t| current_time.saturating_sub(t.timestamp) < max_age_secs);
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}
