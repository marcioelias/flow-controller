//! Address classification shared by labels (Rust) and filters (ClickHouse SQL),
//! built from OWN_ASN_LIST / INTERNAL_PREFIXES (task 17.7).

use std::net::IpAddr;

const CGNAT: &str = "100.64.0.0/10";

/// Private/local space that is always internal, regardless of settings
const BUILTIN_INTERNAL: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "fc00::/7",
    "fe80::/10",
    "::1/128",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cidr {
    addr: IpAddr,
    len: u8,
}

impl Cidr {
    fn parse(s: &str) -> Option<Self> {
        let (addr, len) = match s.trim().split_once('/') {
            Some((a, l)) => (a.parse::<IpAddr>().ok()?, l.parse::<u8>().ok()?),
            None => {
                let a = s.trim().parse::<IpAddr>().ok()?;
                (a, if a.is_ipv4() { 32 } else { 128 })
            }
        };
        let max = if addr.is_ipv4() { 32 } else { 128 };
        (len <= max).then_some(Self { addr, len })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = u32::MAX.checked_shl(32 - self.len as u32).unwrap_or(0);
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = u128::MAX.checked_shl(128 - self.len as u32).unwrap_or(0);
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }

    /// Safe to embed in SQL: rebuilt from parsed parts, never from raw input
    fn sql(&self) -> String {
        format!("'{}/{}'", self.addr, self.len)
    }
}

#[derive(Debug, Clone)]
pub struct NetClassifier {
    own_asns: Vec<u32>,
    internal: Vec<Cidr>,
    cgnat: Cidr,
}

impl NetClassifier {
    pub fn new(own_asn_list: &str, internal_prefixes: &str) -> Self {
        let own_asns = own_asn_list
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter_map(|s| {
                let asn = s.parse::<u32>().ok();
                if asn.is_none() {
                    tracing::warn!("OWN_ASN_LIST: ignoring invalid ASN {s:?}");
                }
                asn
            })
            .collect();

        let mut internal: Vec<Cidr> = BUILTIN_INTERNAL
            .iter()
            .filter_map(|c| Cidr::parse(c))
            .collect();
        for raw in internal_prefixes
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            match Cidr::parse(raw) {
                Some(c) if !internal.contains(&c) => internal.push(c),
                Some(_) => {}
                None => tracing::warn!("INTERNAL_PREFIXES: ignoring invalid CIDR {raw:?}"),
            }
        }

        Self {
            own_asns,
            internal,
            cgnat: Cidr::parse(CGNAT).expect("valid CGNAT range"),
        }
    }

    pub async fn load(pool: &sqlx::SqlitePool) -> Self {
        let asns = crate::settings::get_value(pool, "OWN_ASN_LIST")
            .await
            .unwrap_or_default();
        let prefixes = crate::settings::get_value(pool, "INTERNAL_PREFIXES")
            .await
            .unwrap_or_default();
        Self::new(&asns, &prefixes)
    }

    /// `cgnat` | `internal` | `internet` (R-01 precedence)
    pub fn classify(&self, ip: IpAddr, asn: u32) -> &'static str {
        if self.cgnat.contains(ip) {
            "cgnat"
        } else if (asn != 0 && self.own_asns.contains(&asn))
            || self.internal.iter().any(|c| c.contains(ip))
        {
            "internal"
        } else {
            "internet"
        }
    }

    pub fn classify_str(&self, ip: &str, asn: u32) -> &'static str {
        ip.parse()
            .map(|ip| self.classify(ip, asn))
            .unwrap_or("internet")
    }

    /// ClickHouse boolean: true when the address is cgnat or internal.
    /// `ip_expr` must be a native IPv4/IPv6 column expression of the table family.
    pub fn sql_internal(&self, ip_expr: &str, asn_expr: &str, v6: bool) -> String {
        let mut parts: Vec<String> = std::iter::once(&self.cgnat)
            .chain(self.internal.iter())
            .filter(|c| c.addr.is_ipv6() == v6)
            .map(|c| format!("isIPAddressInRange(toString({ip_expr}), {})", c.sql()))
            .collect();
        if !self.own_asns.is_empty() {
            let list = self
                .own_asns
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("{asn_expr} IN ({list})"));
        }
        format!("({})", parts.join(" OR "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn classifies_by_precedence() {
        let c = NetClassifier::new("52977", "170.231.0.0/16");
        assert_eq!(c.classify(ip("100.68.29.12"), 0), "cgnat");
        assert_eq!(c.classify(ip("170.231.6.78"), 0), "internal");
        assert_eq!(c.classify(ip("200.1.2.3"), 52977), "internal");
        assert_eq!(c.classify(ip("8.8.8.8"), 15169), "internet");
        assert_eq!(c.classify(ip("fd00::1"), 0), "internal");
    }

    #[test]
    fn works_without_settings() {
        let c = NetClassifier::new("", "");
        assert_eq!(c.classify(ip("100.64.0.1"), 0), "cgnat");
        assert_eq!(c.classify(ip("192.168.1.1"), 0), "internal");
        assert_eq!(c.classify(ip("1.1.1.1"), 0), "internet");
    }

    #[test]
    fn invalid_entries_are_dropped_from_sql() {
        let c = NetClassifier::new("12, abc, 34", "10.9.0.0/16, lixo, 1.2.3.4/40, '; DROP");
        let sql = c.sql_internal("dst_ip", "dst_asn", false);
        assert!(sql.contains("'10.9.0.0/16'"));
        assert!(sql.contains("dst_asn IN (12, 34)"));
        assert!(!sql.contains("lixo") && !sql.contains("DROP") && !sql.contains("/40"));
        assert!(!sql.contains("fc00"), "v4 table gets only v4 ranges");
    }
}
