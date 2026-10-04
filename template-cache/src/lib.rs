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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateField {
    pub field_type: u16,
    pub length: u16,
}

/// A parsed template representing a schema for a flow record
#[derive(Debug, Clone)]
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
    /// Decode state changed since the last `take_changed` (task 17.13)
    changed: bool,
    /// Data sets dropped because their template is unknown
    missing_template: u64,
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
            changed: false,
            missing_template: 0,
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
            self.changed = true;
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
        let prev = self.sys_inits.insert((exporter_ip, source_id), ms);
        // Derived from export time − uptime: ignore millisecond jitter
        if prev.is_none_or(|p| p.abs_diff(ms) > 1_000) {
            self.changed = true;
        }
    }

    pub fn sys_inits(&self) -> impl Iterator<Item = (&(Ipv4Addr, u32), &u64)> {
        self.sys_inits.iter()
    }

    pub fn get(&self, key: &TemplateKey) -> Option<&Template> {
        self.cache.get(key)
    }

    pub fn insert(&mut self, mut template: Template) {
        template.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Periodic identical resends refresh the timestamp but aren't a change
        let same = self.cache.get(&template.key).is_some_and(|t| {
            t.fields == template.fields
                && t.is_options == template.is_options
                && t.scope_field_count == template.scope_field_count
        });
        if !same {
            self.changed = true;
        }
        self.cache.insert(template.key, template);
    }

    /// Loads persisted state without marking it as a change (task 17.13 R-02)
    pub fn restore(
        &mut self,
        templates: Vec<Template>,
        sampling: Vec<((Ipv4Addr, u32), u32)>,
        sys_inits: Vec<((Ipv4Addr, u32), u64)>,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        for mut t in templates {
            t.timestamp = now;
            self.cache.entry(t.key).or_insert(t);
        }
        for (k, rate) in sampling {
            self.sampling_rates.entry(k).or_insert(rate.max(1));
        }
        for (k, ms) in sys_inits {
            self.sys_inits.entry(k).or_insert(ms);
        }
    }

    pub fn templates(&self) -> impl Iterator<Item = &Template> {
        self.cache.values()
    }

    /// True once per change of templates, sampling rates or boot times
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    pub fn note_missing_template(&mut self) {
        self.missing_template += 1;
    }

    pub fn take_missing_templates(&mut self) -> u64 {
        std::mem::take(&mut self.missing_template)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn template(len: u16) -> Template {
        Template {
            key: TemplateKey {
                exporter_ip: Ipv4Addr::new(10, 0, 0, 1),
                source_id: 1,
                template_id: 300,
            },
            fields: vec![TemplateField {
                field_type: 8,
                length: len,
            }],
            is_options: false,
            scope_field_count: 0,
            timestamp: 0,
        }
    }

    // AC-02 (task 17.13)
    #[test]
    fn identical_resend_is_not_a_change() {
        let mut c = ThreadLocalTemplateCache::new();
        c.insert(template(4));
        assert!(c.take_changed());
        c.insert(template(4));
        assert!(!c.take_changed());
        c.insert(template(16));
        assert!(c.take_changed());
    }

    #[test]
    fn restore_does_not_mark_change_nor_override_live_state() {
        let mut c = ThreadLocalTemplateCache::new();
        let ip = Ipv4Addr::new(10, 0, 0, 1);
        c.set_sampling_rate(ip, 1, 1000);
        c.take_changed();
        c.restore(
            vec![template(4)],
            vec![((ip, 1), 64), ((ip, 2), 1000)],
            vec![],
        );
        assert!(!c.take_changed());
        assert_eq!(c.sampling_rate(ip, 1), 1000);
        assert_eq!(c.sampling_rate(ip, 2), 1000);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn boot_time_jitter_is_ignored() {
        let mut c = ThreadLocalTemplateCache::new();
        let ip = Ipv4Addr::new(10, 0, 0, 1);
        c.set_sys_init_ms(ip, 1, 1_000_000);
        assert!(c.take_changed());
        c.set_sys_init_ms(ip, 1, 1_000_400);
        assert!(!c.take_changed());
        c.set_sys_init_ms(ip, 1, 2_000_000);
        assert!(c.take_changed());
    }
}
