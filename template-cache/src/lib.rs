use std::collections::HashMap;
use std::net::Ipv4Addr;
use ahash::RandomState;

/// The template key to identify unique Netflow v9 / IPFIX templates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateKey {
    pub exporter_ip: Ipv4Addr,
    pub source_id: u32,  // Observation Domain ID in IPFIX
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
    pub timestamp: u64, // Used for expiration
}

/// A highly-performant cache designed to be owned by a single Worker thread.
/// Because Dispatcher hashes packets by exporter IP, worker threads don't need locks around this structure.
pub struct ThreadLocalTemplateCache {
    cache: HashMap<TemplateKey, Template, RandomState>,
}

impl ThreadLocalTemplateCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::with_hasher(RandomState::new()),
        }
    }

    pub fn get(&self, key: &TemplateKey) -> Option<&Template> {
        self.cache.get(key)
    }

    pub fn insert(&mut self, template: Template) {
        self.cache.insert(template.key, template);
    }

    pub fn prune_old_templates(&mut self, current_time: u64, max_age_secs: u64) {
        self.cache.retain(|_, t| current_time.saturating_sub(t.timestamp) < max_age_secs);
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }
}
