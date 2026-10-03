use byteorder::{BigEndian, ReadBytesExt};
use flow_types::NormalizedFlow;
use std::io::Cursor;
use template_cache::ThreadLocalTemplateCache;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Incomplete packet")]
    Incomplete,
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u16),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
}

// ─── NetFlow v9 header ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct V9Header {
    pub version: u16,
    pub count: u16,
    pub sys_uptime: u32,
    pub unix_secs: u32,
    pub seq_num: u32,
    pub source_id: u32,
}

pub fn parse_v9_header(cursor: &mut Cursor<&[u8]>) -> Result<V9Header, ParseError> {
    let version = cursor.read_u16::<BigEndian>()?;
    if version != 9 {
        return Err(ParseError::UnsupportedVersion(version));
    }
    Ok(V9Header {
        version,
        count: cursor.read_u16::<BigEndian>()?,
        sys_uptime: cursor.read_u32::<BigEndian>()?,
        unix_secs: cursor.read_u32::<BigEndian>()?,
        seq_num: cursor.read_u32::<BigEndian>()?,
        source_id: cursor.read_u32::<BigEndian>()?,
    })
}

// ─── IPFIX (v10) header ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct IpfixHeader {
    pub version: u16,
    pub length: u16,
    pub export_time: u32,
    pub seq_num: u32,
    pub observation_domain_id: u32,
}

fn parse_ipfix_header(cursor: &mut Cursor<&[u8]>) -> Result<IpfixHeader, ParseError> {
    Ok(IpfixHeader {
        version: cursor.read_u16::<BigEndian>()?,
        length: cursor.read_u16::<BigEndian>()?,
        export_time: cursor.read_u32::<BigEndian>()?,
        seq_num: cursor.read_u32::<BigEndian>()?,
        observation_domain_id: cursor.read_u32::<BigEndian>()?,
    })
}

// ─── IANA field IDs (shared by v9 and IPFIX) ─────────────────────────────────

pub const IANA_IN_BYTES: u16 = 1;
pub const IANA_IN_PKTS: u16 = 2;
pub const IANA_PROTOCOL: u16 = 4;
pub const IANA_TCP_FLAGS: u16 = 6;
pub const IANA_L4_SRC_PORT: u16 = 7;
pub const IANA_IPV4_SRC_ADDR: u16 = 8;
pub const IANA_INGRESS_IFACE: u16 = 10;
pub const IANA_L4_DST_PORT: u16 = 11;
pub const IANA_IPV4_DST_ADDR: u16 = 12;
pub const IANA_EGRESS_IFACE: u16 = 14;
/// 0 = ingress, 1 = egress (router perspective)
pub const IANA_FLOW_DIRECTION: u16 = 61;
// Post-NAT (task 17.8)
pub const IANA_POST_NAT_SRC_IPV4: u16 = 225;
pub const IANA_POST_NAT_DST_IPV4: u16 = 226;
pub const IANA_POST_NAPT_SRC_PORT: u16 = 227;
pub const IANA_POST_NAPT_DST_PORT: u16 = 228;
pub const IANA_POST_NAT_SRC_IPV6: u16 = 281;
pub const IANA_POST_NAT_DST_IPV6: u16 = 282;
// Cisco NSEL (NetFlow v9 only; IDs above 32767 are valid in v9)
pub const NSEL_XLATE_SRC_ADDR_IPV4: u16 = 40001;
pub const NSEL_XLATE_DST_ADDR_IPV4: u16 = 40002;
pub const NSEL_XLATE_SRC_PORT: u16 = 40003;
pub const NSEL_XLATE_DST_PORT: u16 = 40004;
pub const IANA_BGP_SRC_ASN: u16 = 16;
pub const IANA_BGP_DST_ASN: u16 = 17;
pub const IANA_IPV6_SRC_ADDR: u16 = 27;
pub const IANA_IPV6_DST_ADDR: u16 = 28;

// Flow timestamp IEs (task 13.1)
pub const IANA_LAST_SWITCHED: u16 = 21; // v9 — ms relative to header sysUptime
pub const IANA_FIRST_SWITCHED: u16 = 22; // v9 — ms relative to header sysUptime
pub const IANA_FLOW_END_SEC: u16 = 151; // IPFIX — unix seconds
pub const IANA_FLOW_START_SEC: u16 = 150; // IPFIX — unix seconds
pub const IANA_FLOW_END_MS: u16 = 153; // IPFIX — unix milliseconds
pub const IANA_FLOW_START_MS: u16 = 152; // IPFIX — unix milliseconds
pub const IANA_FLOW_START_US: u16 = 154; // IPFIX — NTP 64-bit (RFC 7011 §6.1.10)
pub const IANA_FLOW_END_US: u16 = 155; // IPFIX — NTP 64-bit

// Sampling-related IEs carried in options data records
pub const IANA_SAMPLING_INTERVAL: u16 = 34; // v9 classic
pub const IANA_SAMPLER_RANDOM_INTERVAL: u16 = 50; // v9 random sampler
pub const IANA_SAMPLING_PKT_INTERVAL: u16 = 305; // IPFIX
pub const IANA_SAMPLING_PKT_SPACE: u16 = 306; // IPFIX — rate = (interval+space)/interval
pub const IANA_SYSTEM_INIT_TIME_MS: u16 = 160; // IPFIX options — unix ms do boot

/// IPFIX variable-length field marker (RFC 7011 §7): actual length is a
/// per-record prefix, not part of the template.
pub const VARLEN: u16 = 0xFFFF;

#[inline]
fn read_uint(bytes: &[u8]) -> u64 {
    // Fields wider than 8 bytes are not numeric counters we understand;
    // take the low-order 8 bytes rather than silently shifting them out.
    let start = bytes.len().saturating_sub(8);
    bytes[start..]
        .iter()
        .fold(0u64, |acc, &b| (acc << 8) | b as u64)
}

// ─── Shared template/data set parsers ────────────────────────────────────────

fn exporter_v4(exporter_ip: std::net::IpAddr) -> std::net::Ipv4Addr {
    match exporter_ip {
        std::net::IpAddr::V4(v4) => v4,
        std::net::IpAddr::V6(_) => std::net::Ipv4Addr::new(0, 0, 0, 0),
    }
}

/// Parse a template flowset/set (flowset_id 0 for v9, set_id 2 for IPFIX).
/// Returns after consuming `set_data` which is the payload inside the set (after the 4-byte header).
fn parse_template_set(
    set_data: &[u8],
    source_id: u32,
    export_time: u32,
    exporter_ip: std::net::Ipv4Addr,
    templates: &mut ThreadLocalTemplateCache,
    enterprise_ids: bool, // true for IPFIX (may have enterprise bit)
) {
    let mut ptr = 0usize;
    while ptr + 4 <= set_data.len() {
        let template_id = u16::from_be_bytes([set_data[ptr], set_data[ptr + 1]]);
        let field_count = u16::from_be_bytes([set_data[ptr + 2], set_data[ptr + 3]]) as usize;
        ptr += 4;

        // field_count comes off the wire — clamp the allocation to what the
        // remaining bytes could actually describe (4 bytes per field entry)
        let max_possible = (set_data.len() - ptr) / 4;
        let mut fields = Vec::with_capacity(field_count.min(max_possible));
        for _ in 0..field_count {
            if ptr + 4 > set_data.len() {
                break;
            }
            let raw_type = u16::from_be_bytes([set_data[ptr], set_data[ptr + 1]]);
            let length = u16::from_be_bytes([set_data[ptr + 2], set_data[ptr + 3]]);
            ptr += 4;

            if enterprise_ids && (raw_type & 0x8000) != 0 {
                // Enterprise bit set: skip 4-byte enterprise number, don't store this field
                ptr += 4;
                continue;
            }

            // v9 has no enterprise bit: IDs above 32767 (Cisco NSEL) are real
            fields.push(template_cache::TemplateField {
                field_type: if enterprise_ids {
                    raw_type & 0x7FFF
                } else {
                    raw_type
                },
                length,
            });
        }

        if template_id > 255 {
            templates.insert(template_cache::Template {
                key: template_cache::TemplateKey {
                    exporter_ip,
                    source_id,
                    template_id,
                },
                fields,
                is_options: false,
                scope_field_count: 0,
                timestamp: export_time as u64,
            });
        }
    }
}

/// Parse a NetFlow v9 options template flowset (flowset_id 1).
/// Layout: template_id (u16), option_scope_length (u16, bytes),
/// option_length (u16, bytes), then scope entries then option entries,
/// each `type (u16), length (u16)`.
fn parse_v9_options_template_set(
    set_data: &[u8],
    source_id: u32,
    export_time: u32,
    exporter_ip: std::net::Ipv4Addr,
    templates: &mut ThreadLocalTemplateCache,
) {
    let mut ptr = 0usize;
    while ptr + 6 <= set_data.len() {
        let template_id = u16::from_be_bytes([set_data[ptr], set_data[ptr + 1]]);
        let scope_len = u16::from_be_bytes([set_data[ptr + 2], set_data[ptr + 3]]) as usize;
        let option_len = u16::from_be_bytes([set_data[ptr + 4], set_data[ptr + 5]]) as usize;
        ptr += 6;

        if ptr + scope_len + option_len > set_data.len() {
            break;
        }

        let scope_field_count = (scope_len / 4) as u16;
        let total_fields = (scope_len + option_len) / 4;
        let mut fields = Vec::with_capacity(total_fields);
        for _ in 0..total_fields {
            if ptr + 4 > set_data.len() {
                break;
            }
            let field_type = u16::from_be_bytes([set_data[ptr], set_data[ptr + 1]]);
            let length = u16::from_be_bytes([set_data[ptr + 2], set_data[ptr + 3]]);
            ptr += 4;
            fields.push(template_cache::TemplateField { field_type, length });
        }

        if template_id > 255 {
            templates.insert(template_cache::Template {
                key: template_cache::TemplateKey {
                    exporter_ip,
                    source_id,
                    template_id,
                },
                fields,
                is_options: true,
                scope_field_count,
                timestamp: export_time as u64,
            });
        }
    }
}

/// Parse an IPFIX options template set (set_id 3).
/// Layout: template_id (u16), field_count (u16), scope_field_count (u16),
/// then all field specs (enterprise bit handled as in regular templates).
fn parse_ipfix_options_template_set(
    set_data: &[u8],
    source_id: u32,
    export_time: u32,
    exporter_ip: std::net::Ipv4Addr,
    templates: &mut ThreadLocalTemplateCache,
) {
    let mut ptr = 0usize;
    while ptr + 6 <= set_data.len() {
        let template_id = u16::from_be_bytes([set_data[ptr], set_data[ptr + 1]]);
        let field_count = u16::from_be_bytes([set_data[ptr + 2], set_data[ptr + 3]]) as usize;
        let scope_field_count = u16::from_be_bytes([set_data[ptr + 4], set_data[ptr + 5]]);
        ptr += 6;

        let max_possible = (set_data.len() - ptr) / 4;
        let mut fields = Vec::with_capacity(field_count.min(max_possible));
        for _ in 0..field_count {
            if ptr + 4 > set_data.len() {
                break;
            }
            let raw_type = u16::from_be_bytes([set_data[ptr], set_data[ptr + 1]]);
            let length = u16::from_be_bytes([set_data[ptr + 2], set_data[ptr + 3]]);
            ptr += 4;

            if (raw_type & 0x8000) != 0 {
                // Enterprise bit: skip the 4-byte enterprise number but keep the
                // field for record walking — length still counts in the record
                ptr += 4;
            }

            fields.push(template_cache::TemplateField {
                field_type: raw_type & 0x7FFF,
                length,
            });
        }

        if template_id > 255 {
            templates.insert(template_cache::Template {
                key: template_cache::TemplateKey {
                    exporter_ip,
                    source_id,
                    template_id,
                },
                fields,
                is_options: true,
                scope_field_count,
                timestamp: export_time as u64,
            });
        }
    }
}

/// Decode an options data record set: extract sampling-related IEs.
/// Returns the sampling rate found, if any; produces no flows.
fn parse_options_data_set(
    set_data: &[u8],
    template: &template_cache::Template,
) -> (Option<u32>, Option<u64>) {
    let record_size: usize = template.fields.iter().map(|f| f.length as usize).sum();
    if record_size == 0 || template.fields.iter().any(|f| f.length == VARLEN) {
        return (None, None);
    }

    let mut learned: Option<u32> = None;
    let mut sys_init: Option<u64> = None;
    let mut d_ptr = 0usize;

    while d_ptr + record_size <= set_data.len() {
        let mut f_ptr = d_ptr;
        let mut sampling_interval: Option<u64> = None;
        let mut random_interval: Option<u64> = None;
        let mut pkt_interval: Option<u64> = None;
        let mut pkt_space: Option<u64> = None;

        for field in &template.fields {
            let f_len = field.length as usize;
            if f_ptr + f_len > set_data.len() {
                break;
            }
            let field_data = &set_data[f_ptr..f_ptr + f_len];
            match field.field_type {
                IANA_SAMPLING_INTERVAL => sampling_interval = Some(read_uint(field_data)),
                IANA_SAMPLER_RANDOM_INTERVAL => random_interval = Some(read_uint(field_data)),
                IANA_SAMPLING_PKT_INTERVAL => pkt_interval = Some(read_uint(field_data)),
                IANA_SAMPLING_PKT_SPACE => pkt_space = Some(read_uint(field_data)),
                IANA_SYSTEM_INIT_TIME_MS => sys_init = Some(read_uint(field_data)),
                _ => {}
            }
            f_ptr += f_len;
        }

        // Precedence: v9 classic > v9 random > IPFIX interval/space
        let rate = sampling_interval.or(random_interval).or_else(|| {
            pkt_interval.filter(|&i| i > 0).map(|i| {
                // rate = (interval + space) / interval; space absent → unsampled
                (i + pkt_space.unwrap_or(0)) / i
            })
        });

        if let Some(r) = rate {
            if r >= 1 && r <= u32::MAX as u64 {
                learned = Some(r as u32);
            }
        }

        d_ptr += record_size;
    }

    (learned, sys_init)
}

/// Header clock context needed to resolve flow timestamps.
/// v9 reports FIRST/LAST_SWITCHED relative to the exporter's sysUptime;
/// IPFIX reports absolute time. Both need the export moment as anchor.
#[derive(Clone, Copy)]
pub struct TimeCtx {
    /// Export moment in unix ms (header unix_secs / export_time)
    pub export_ms: u64,
    /// Header sysUptime in ms — Some only for NetFlow v9
    pub uptime_ms: Option<u32>,
    /// systemInitTimeMilliseconds (IE 160) aprendido via options — base para
    /// IE 21/22 em IPFIX, que não tem sysUptime no header
    pub sys_init_ms: Option<u64>,
}

/// Seconds between the NTP epoch (1900) and the unix epoch (1970)
const NTP_UNIX_OFFSET_SECS: u64 = 2_208_988_800;

/// Convert a v9 sysUptime-relative value to unix ms, robust to the 32-bit
/// uptime wrap (~49.7 days): the age is computed with wrapping arithmetic.
#[inline]
fn v9_uptime_to_unix_ms(raw_ms: u64, ctx: &TimeCtx) -> u64 {
    match ctx.uptime_ms {
        Some(uptime) => {
            let age_ms = uptime.wrapping_sub(raw_ms as u32) as u64;
            ctx.export_ms.saturating_sub(age_ms)
        }
        None => 0,
    }
}

/// Convert an RFC 7011 dateTimeMicroseconds (64-bit NTP) value to unix ms
#[inline]
fn ntp64_to_unix_ms(raw: u64) -> u64 {
    let secs = (raw >> 32).saturating_sub(NTP_UNIX_OFFSET_SECS);
    let frac_ms = ((raw & 0xFFFF_FFFF) * 1000) >> 32;
    secs * 1000 + frac_ms
}

#[inline]
fn empty_flow(export_time: u32, exporter_ip: std::net::Ipv4Addr) -> NormalizedFlow {
    NormalizedFlow {
        timestamp: export_time as u64,
        exporter_ip,
        src_ip: flow_types::IpAddrType::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
        dst_ip: flow_types::IpAddrType::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
        src_port: 0,
        dst_port: 0,
        protocol: 0,
        bytes: 0,
        packets: 0,
        src_asn: 0,
        dst_asn: 0,
        ingress_interface: 0,
        egress_interface: 0,
        tcp_flags: 0,
        sampling_rate: 1,
        direction: flow_types::DIRECTION_UNKNOWN,
        start_ms: 0,
        end_ms: 0,
        nat_src_ip: None,
        nat_dst_ip: None,
        nat_src_port: 0,
        nat_dst_port: 0,
    }
}

fn read_ip(field_data: &[u8]) -> Option<flow_types::IpAddrType> {
    match field_data.len() {
        4 => {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(field_data);
            Some(flow_types::IpAddrType::V4(std::net::Ipv4Addr::from(arr)))
        }
        16 => {
            let mut arr = [0u8; 16];
            arr.copy_from_slice(field_data);
            Some(flow_types::IpAddrType::V6(std::net::Ipv6Addr::from(arr)))
        }
        _ => None,
    }
}

#[inline]
fn decode_field(flow: &mut NormalizedFlow, field_type: u16, field_data: &[u8]) {
    let f_len = field_data.len();
    match field_type {
        IANA_FLOW_START_SEC => flow.start_ms = read_uint(field_data) * 1000,
        IANA_FLOW_END_SEC => flow.end_ms = read_uint(field_data) * 1000,
        IANA_FLOW_START_MS => flow.start_ms = read_uint(field_data),
        IANA_FLOW_END_MS => flow.end_ms = read_uint(field_data),
        IANA_FLOW_START_US => flow.start_ms = ntp64_to_unix_ms(read_uint(field_data)),
        IANA_FLOW_END_US => flow.end_ms = ntp64_to_unix_ms(read_uint(field_data)),
        IANA_IN_BYTES => flow.bytes = read_uint(field_data),
        IANA_IN_PKTS => flow.packets = read_uint(field_data),
        IANA_PROTOCOL => {
            if f_len == 1 {
                flow.protocol = field_data[0];
            }
        }
        IANA_TCP_FLAGS => {
            if f_len > 0 {
                flow.tcp_flags = field_data[f_len - 1];
            }
        }
        IANA_L4_SRC_PORT => flow.src_port = read_uint(field_data) as u16,
        IANA_L4_DST_PORT => flow.dst_port = read_uint(field_data) as u16,
        IANA_IPV4_SRC_ADDR => {
            if f_len == 4 {
                let mut arr = [0u8; 4];
                arr.copy_from_slice(field_data);
                flow.src_ip = flow_types::IpAddrType::V4(std::net::Ipv4Addr::from(arr));
            }
        }
        IANA_IPV4_DST_ADDR => {
            if f_len == 4 {
                let mut arr = [0u8; 4];
                arr.copy_from_slice(field_data);
                flow.dst_ip = flow_types::IpAddrType::V4(std::net::Ipv4Addr::from(arr));
            }
        }
        IANA_IPV6_SRC_ADDR => {
            if f_len == 16 {
                let mut arr = [0u8; 16];
                arr.copy_from_slice(field_data);
                flow.src_ip = flow_types::IpAddrType::V6(std::net::Ipv6Addr::from(arr));
            }
        }
        IANA_IPV6_DST_ADDR => {
            if f_len == 16 {
                let mut arr = [0u8; 16];
                arr.copy_from_slice(field_data);
                flow.dst_ip = flow_types::IpAddrType::V6(std::net::Ipv6Addr::from(arr));
            }
        }
        IANA_BGP_SRC_ASN => flow.src_asn = read_uint(field_data) as u32,
        IANA_BGP_DST_ASN => flow.dst_asn = read_uint(field_data) as u32,
        IANA_INGRESS_IFACE => flow.ingress_interface = read_uint(field_data) as u32,
        IANA_EGRESS_IFACE => flow.egress_interface = read_uint(field_data) as u32,
        IANA_FLOW_DIRECTION => {
            if f_len == 1 && field_data[0] <= 1 {
                flow.direction = field_data[0];
            }
        }
        IANA_POST_NAT_SRC_IPV4 | IANA_POST_NAT_SRC_IPV6 | NSEL_XLATE_SRC_ADDR_IPV4 => {
            flow.nat_src_ip = read_ip(field_data).or(flow.nat_src_ip);
        }
        IANA_POST_NAT_DST_IPV4 | IANA_POST_NAT_DST_IPV6 | NSEL_XLATE_DST_ADDR_IPV4 => {
            flow.nat_dst_ip = read_ip(field_data).or(flow.nat_dst_ip);
        }
        IANA_POST_NAPT_SRC_PORT | NSEL_XLATE_SRC_PORT => {
            flow.nat_src_port = read_uint(field_data) as u16
        }
        IANA_POST_NAPT_DST_PORT | NSEL_XLATE_DST_PORT => {
            flow.nat_dst_port = read_uint(field_data) as u16
        }
        _ => {}
    }
}

/// IE 21/22 (FIRST/LAST_SWITCHED) são relativos e precisam de contexto:
/// v9 usa o sysUptime do header; IPFIX usa IE 160 aprendido via options; sem
/// nenhum dos dois, ancora o fim no export e preserva a duração exata
/// (last − first), que independe da base de tempo.
#[inline]
fn resolve_switched_times(
    flow: &mut NormalizedFlow,
    raw_first: Option<u64>,
    raw_last: Option<u64>,
    time: &TimeCtx,
) {
    // IEs absolutos (150–155), se presentes, têm precedência
    if flow.start_ms != 0 || flow.end_ms != 0 {
        return;
    }
    let (Some(first), Some(last)) = (raw_first, raw_last) else {
        return;
    };
    if time.uptime_ms.is_some() {
        flow.start_ms = v9_uptime_to_unix_ms(first, time);
        flow.end_ms = v9_uptime_to_unix_ms(last, time);
    } else if let Some(init) = time.sys_init_ms {
        flow.start_ms = init.saturating_add(first);
        flow.end_ms = init.saturating_add(last);
    } else {
        let duration_ms = (last as u32).wrapping_sub(first as u32) as u64;
        flow.end_ms = time.export_ms;
        flow.start_ms = time.export_ms.saturating_sub(duration_ms);
    }
}

/// Parse a data set and return decoded NormalizedFlow records.
fn parse_data_set(
    set_data: &[u8],
    template_id: u16,
    source_id: u32,
    export_time: u32,
    exporter_ip: std::net::Ipv4Addr,
    templates: &ThreadLocalTemplateCache,
    time: &TimeCtx,
) -> Vec<NormalizedFlow> {
    let key = template_cache::TemplateKey {
        exporter_ip,
        source_id,
        template_id,
    };

    let template = match templates.get(&key) {
        Some(t) => t,
        None => return vec![],
    };

    // Sampled exporter: scale counters so stored volumes approximate reality
    let sampling_rate = templates.sampling_rate(exporter_ip, source_id);

    if template.fields.iter().any(|f| f.length == VARLEN) {
        return parse_varlen_records(
            set_data,
            template,
            export_time,
            exporter_ip,
            sampling_rate,
            time,
        );
    }

    let record_size: usize = template.fields.iter().map(|f| f.length as usize).sum();
    if record_size == 0 {
        return vec![];
    }

    let mut flows = Vec::new();
    let mut d_ptr = 0usize;

    while d_ptr + record_size <= set_data.len() {
        let mut flow = empty_flow(export_time, exporter_ip);

        let mut f_ptr = d_ptr;
        let mut raw_first: Option<u64> = None;
        let mut raw_last: Option<u64> = None;
        for field in &template.fields {
            let f_len = field.length as usize;
            if f_ptr + f_len > set_data.len() {
                break;
            }
            let field_data = &set_data[f_ptr..f_ptr + f_len];
            match field.field_type {
                IANA_FIRST_SWITCHED => raw_first = Some(read_uint(field_data)),
                IANA_LAST_SWITCHED => raw_last = Some(read_uint(field_data)),
                _ => decode_field(&mut flow, field.field_type, field_data),
            }
            f_ptr += f_len;
        }

        resolve_switched_times(&mut flow, raw_first, raw_last, time);
        apply_sampling(&mut flow, sampling_rate);
        flows.push(flow);
        d_ptr += record_size;
    }

    flows
}

#[inline]
fn apply_sampling(flow: &mut NormalizedFlow, rate: u32) {
    flow.sampling_rate = rate;
    if rate > 1 {
        flow.bytes = flow.bytes.saturating_mul(rate as u64);
        flow.packets = flow.packets.saturating_mul(rate as u64);
    }
}

/// Data set whose template contains variable-length IEs (RFC 7011 §7):
/// record boundaries can only be found by walking each record field by field,
/// reading the 1-byte (or 255 + 2-byte) length prefix of every varlen field.
/// Varlen values themselves are not among the IANA IDs we decode — they are
/// skipped — but the fixed-length fields around them parse normally.
fn parse_varlen_records(
    set_data: &[u8],
    template: &template_cache::Template,
    export_time: u32,
    exporter_ip: std::net::Ipv4Addr,
    sampling_rate: u32,
    time: &TimeCtx,
) -> Vec<NormalizedFlow> {
    // Smallest possible record: fixed lengths + 1 prefix byte per varlen field.
    // Anything shorter at the tail is set padding, not a record.
    let min_record: usize = template
        .fields
        .iter()
        .map(|f| {
            if f.length == VARLEN {
                1
            } else {
                f.length as usize
            }
        })
        .sum();
    if min_record == 0 {
        return vec![];
    }

    let mut flows = Vec::new();
    let mut d_ptr = 0usize;

    'records: while d_ptr + min_record <= set_data.len() {
        let mut flow = empty_flow(export_time, exporter_ip);

        let mut f_ptr = d_ptr;
        let mut raw_first: Option<u64> = None;
        let mut raw_last: Option<u64> = None;
        for field in &template.fields {
            let f_len = if field.length == VARLEN {
                if f_ptr >= set_data.len() {
                    break 'records;
                }
                let short = set_data[f_ptr] as usize;
                f_ptr += 1;
                if short == 255 {
                    if f_ptr + 2 > set_data.len() {
                        break 'records;
                    }
                    let long = u16::from_be_bytes([set_data[f_ptr], set_data[f_ptr + 1]]) as usize;
                    f_ptr += 2;
                    long
                } else {
                    short
                }
            } else {
                field.length as usize
            };

            if f_ptr + f_len > set_data.len() {
                break 'records;
            }
            let field_data = &set_data[f_ptr..f_ptr + f_len];
            match field.field_type {
                IANA_FIRST_SWITCHED => raw_first = Some(read_uint(field_data)),
                IANA_LAST_SWITCHED => raw_last = Some(read_uint(field_data)),
                _ => decode_field(&mut flow, field.field_type, field_data),
            }
            f_ptr += f_len;
        }

        resolve_switched_times(&mut flow, raw_first, raw_last, time);
        apply_sampling(&mut flow, sampling_rate);
        flows.push(flow);
        d_ptr = f_ptr;
    }

    flows
}

/// Route a data set: options templates feed the sampling cache; regular
/// templates produce flows. Shared by v9 and IPFIX.
#[allow(clippy::too_many_arguments)]
fn handle_data_set(
    set_data: &[u8],
    template_id: u16,
    source_id: u32,
    export_time: u32,
    exporter_ip: std::net::Ipv4Addr,
    templates: &mut ThreadLocalTemplateCache,
    flows: &mut Vec<NormalizedFlow>,
    time: &TimeCtx,
) {
    let key = template_cache::TemplateKey {
        exporter_ip,
        source_id,
        template_id,
    };

    // Options data records update the sampling rate; they carry no flows.
    // Read in one scope so the mutable cache update can happen after.
    let (learned_rate, learned_sys_init) = match templates.get(&key) {
        Some(t) if t.is_options => parse_options_data_set(set_data, t),
        Some(_) => {
            // IE 160 pode ter chegado neste mesmo pacote — resolve agora,
            // não no início da mensagem
            let set_time = TimeCtx {
                sys_init_ms: time
                    .sys_init_ms
                    .or_else(|| templates.sys_init_ms(exporter_ip, source_id)),
                ..*time
            };
            let mut data_flows = parse_data_set(
                set_data,
                template_id,
                source_id,
                export_time,
                exporter_ip,
                templates,
                &set_time,
            );
            flows.append(&mut data_flows);
            return;
        }
        None => return,
    };

    if let Some(rate) = learned_rate {
        templates.set_sampling_rate(exporter_ip, source_id, rate);
    }
    if let Some(ms) = learned_sys_init {
        templates.set_sys_init_ms(exporter_ip, source_id, ms);
    }
}

// ─── NetFlow v9 message parser ────────────────────────────────────────────────

fn parse_v9_message(
    payload: &[u8],
    templates: &mut ThreadLocalTemplateCache,
    exporter_ip: std::net::IpAddr,
) -> Result<Vec<NormalizedFlow>, ParseError> {
    let mut cur = Cursor::new(payload);
    cur.set_position(0);
    let hdr = parse_v9_header(&mut cur)?;

    let ipv4_exporter = exporter_v4(exporter_ip);
    let time = TimeCtx {
        export_ms: hdr.unix_secs as u64 * 1000,
        uptime_ms: Some(hdr.sys_uptime),
        sys_init_ms: None,
    };
    let mut flows = Vec::with_capacity(32);
    let mut ptr = 20usize; // v9 header is 20 bytes

    while ptr + 4 <= payload.len() {
        let flowset_id = u16::from_be_bytes([payload[ptr], payload[ptr + 1]]);
        let length = u16::from_be_bytes([payload[ptr + 2], payload[ptr + 3]]) as usize;

        if length < 4 || ptr + length > payload.len() {
            break;
        }

        let set_data = &payload[ptr + 4..ptr + length];

        match flowset_id {
            0 => {
                // Template FlowSet
                parse_template_set(
                    set_data,
                    hdr.source_id,
                    hdr.unix_secs,
                    ipv4_exporter,
                    templates,
                    false,
                );
            }
            1 => {
                // Options Template FlowSet — carries sampling metadata
                parse_v9_options_template_set(
                    set_data,
                    hdr.source_id,
                    hdr.unix_secs,
                    ipv4_exporter,
                    templates,
                );
            }
            id if id > 255 => {
                handle_data_set(
                    set_data,
                    id,
                    hdr.source_id,
                    hdr.unix_secs,
                    ipv4_exporter,
                    templates,
                    &mut flows,
                    &time,
                );
            }
            _ => {}
        }

        ptr += length;
    }

    Ok(flows)
}

// ─── IPFIX (v10) message parser ───────────────────────────────────────────────

fn parse_ipfix_message(
    payload: &[u8],
    templates: &mut ThreadLocalTemplateCache,
    exporter_ip: std::net::IpAddr,
) -> Result<Vec<NormalizedFlow>, ParseError> {
    if payload.len() < 16 {
        return Err(ParseError::Incomplete);
    }

    let mut cur = Cursor::new(payload);
    let hdr = parse_ipfix_header(&mut cur)?;

    let ipv4_exporter = exporter_v4(exporter_ip);
    let time = TimeCtx {
        export_ms: hdr.export_time as u64 * 1000,
        uptime_ms: None,
        sys_init_ms: None, // resolvido por data set (handle_data_set)
    };
    let mut flows = Vec::with_capacity(32);
    let mut ptr = 16usize; // IPFIX header is 16 bytes

    let msg_len = hdr.length as usize;
    let end = msg_len.min(payload.len());

    while ptr + 4 <= end {
        let set_id = u16::from_be_bytes([payload[ptr], payload[ptr + 1]]);
        let set_length = u16::from_be_bytes([payload[ptr + 2], payload[ptr + 3]]) as usize;

        if set_length < 4 || ptr + set_length > end {
            break;
        }

        let set_data = &payload[ptr + 4..ptr + set_length];

        match set_id {
            2 => {
                // IPFIX Template Set (equivalent to v9 flowset 0)
                parse_template_set(
                    set_data,
                    hdr.observation_domain_id,
                    hdr.export_time,
                    ipv4_exporter,
                    templates,
                    true, // enterprise IEs possible
                );
            }
            3 => {
                // Options Template Set — carries sampling metadata
                parse_ipfix_options_template_set(
                    set_data,
                    hdr.observation_domain_id,
                    hdr.export_time,
                    ipv4_exporter,
                    templates,
                );
            }
            id if id >= 256 => {
                handle_data_set(
                    set_data,
                    id,
                    hdr.observation_domain_id,
                    hdr.export_time,
                    ipv4_exporter,
                    templates,
                    &mut flows,
                    &time,
                );
            }
            _ => {}
        }

        ptr += set_length;
    }

    Ok(flows)
}

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn parse_packet(
    payload: &[u8],
    templates: &mut ThreadLocalTemplateCache,
    exporter_ip: std::net::IpAddr,
) -> Result<Vec<NormalizedFlow>, ParseError> {
    if payload.len() < 2 {
        return Err(ParseError::Incomplete);
    }
    let version = u16::from_be_bytes([payload[0], payload[1]]);
    match version {
        9 => parse_v9_message(payload, templates, exporter_ip),
        10 => parse_ipfix_message(payload, templates, exporter_ip),
        v => Err(ParseError::UnsupportedVersion(v)),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use template_cache::ThreadLocalTemplateCache;

    // Build a minimal NetFlow v9 packet: header + template flowset + data flowset
    fn build_v9_packet(src_ip: [u8; 4], dst_ip: [u8; 4], bytes: u64, packets: u64) -> Vec<u8> {
        let mut pkt = Vec::new();

        // v9 Header (20 bytes): version=9, count=2, uptime, unix_secs, seq, source_id
        pkt.extend_from_slice(&9u16.to_be_bytes());
        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&1000u32.to_be_bytes());
        pkt.extend_from_slice(&1700000000u32.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&100u32.to_be_bytes()); // source_id

        // Template FlowSet (id=0)
        // Fields: IN_BYTES(1,8), IN_PKTS(2,8), PROTOCOL(4,1), SRC_PORT(7,2), SRC_ADDR(8,4), DST_PORT(11,2), DST_ADDR(12,4)
        let fields: &[(u16, u16)] = &[
            (IANA_IN_BYTES, 8),
            (IANA_IN_PKTS, 8),
            (IANA_PROTOCOL, 1),
            (IANA_L4_SRC_PORT, 2),
            (IANA_IPV4_SRC_ADDR, 4),
            (IANA_L4_DST_PORT, 2),
            (IANA_IPV4_DST_ADDR, 4),
        ];
        let template_id: u16 = 300;
        let field_count = fields.len() as u16;
        // length = 4 (flowset header) + 4 (template header) + field_count * 4
        let fs_len = 4 + 4 + field_count as usize * 4;
        pkt.extend_from_slice(&0u16.to_be_bytes()); // flowset_id = 0
        pkt.extend_from_slice(&(fs_len as u16).to_be_bytes());
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&field_count.to_be_bytes());
        for (ft, fl) in fields {
            pkt.extend_from_slice(&ft.to_be_bytes());
            pkt.extend_from_slice(&fl.to_be_bytes());
        }

        // Data FlowSet (id=300)
        // Record: bytes(8) + packets(8) + protocol(1) + src_port(2) + src_ip(4) + dst_port(2) + dst_ip(4) = 29 bytes
        let record_size = 8 + 8 + 1 + 2 + 4 + 2 + 4usize;
        let data_len = 4 + record_size;
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(data_len as u16).to_be_bytes());
        pkt.extend_from_slice(&bytes.to_be_bytes());
        pkt.extend_from_slice(&packets.to_be_bytes());
        pkt.push(6u8); // TCP
        pkt.extend_from_slice(&12345u16.to_be_bytes()); // src_port
        pkt.extend_from_slice(&src_ip);
        pkt.extend_from_slice(&443u16.to_be_bytes()); // dst_port
        pkt.extend_from_slice(&dst_ip);

        pkt
    }

    // Build a minimal IPFIX packet: header + template set + data set
    fn build_ipfix_packet(src_ip: [u8; 4], dst_ip: [u8; 4], bytes: u64, packets: u64) -> Vec<u8> {
        let mut pkt = Vec::new();

        let fields: &[(u16, u16)] = &[
            (IANA_IN_BYTES, 8),
            (IANA_IN_PKTS, 8),
            (IANA_PROTOCOL, 1),
            (IANA_L4_SRC_PORT, 2),
            (IANA_IPV4_SRC_ADDR, 4),
            (IANA_L4_DST_PORT, 2),
            (IANA_IPV4_DST_ADDR, 4),
        ];
        let template_id: u16 = 300;
        let field_count = fields.len() as u16;

        // Template set: set_id=2, set_length = 4 + 4 + fields*4
        let tmpl_set_len = 4 + 4 + field_count as usize * 4;
        // Data set: set_id=300, set_length = 4 + record_size
        let record_size = 8 + 8 + 1 + 2 + 4 + 2 + 4usize;
        let data_set_len = 4 + record_size;
        let total_len = 16 + tmpl_set_len + data_set_len;

        // IPFIX Header (16 bytes)
        pkt.extend_from_slice(&10u16.to_be_bytes());
        pkt.extend_from_slice(&(total_len as u16).to_be_bytes());
        pkt.extend_from_slice(&1700000000u32.to_be_bytes()); // export_time
        pkt.extend_from_slice(&1u32.to_be_bytes()); // seq_num
        pkt.extend_from_slice(&100u32.to_be_bytes()); // observation_domain_id

        // Template Set (set_id=2)
        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&(tmpl_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&field_count.to_be_bytes());
        for (ft, fl) in fields {
            pkt.extend_from_slice(&ft.to_be_bytes());
            pkt.extend_from_slice(&fl.to_be_bytes());
        }

        // Data Set (set_id=300)
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(data_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&bytes.to_be_bytes());
        pkt.extend_from_slice(&packets.to_be_bytes());
        pkt.push(6u8);
        pkt.extend_from_slice(&12345u16.to_be_bytes());
        pkt.extend_from_slice(&src_ip);
        pkt.extend_from_slice(&443u16.to_be_bytes());
        pkt.extend_from_slice(&dst_ip);

        pkt
    }

    #[test]
    fn test_v9_parse_flow() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        let pkt = build_v9_packet([192, 168, 1, 10], [8, 8, 8, 8], 4096, 32);

        let flows = parse_packet(&pkt, &mut cache, exporter).expect("parse failed");
        assert_eq!(flows.len(), 1);
        let f = &flows[0];
        assert_eq!(f.bytes, 4096);
        assert_eq!(f.packets, 32);
        assert_eq!(f.protocol, 6);
        assert_eq!(f.dst_port, 443);
        assert!(
            matches!(f.src_ip, flow_types::IpAddrType::V4(ip) if ip == std::net::Ipv4Addr::new(192, 168, 1, 10))
        );
    }

    #[test]
    fn test_ipfix_parse_flow() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 2));
        let pkt = build_ipfix_packet([10, 0, 1, 5], [1, 1, 1, 1], 8192, 64);

        let flows = parse_packet(&pkt, &mut cache, exporter).expect("ipfix parse failed");
        assert_eq!(flows.len(), 1);
        let f = &flows[0];
        assert_eq!(f.bytes, 8192);
        assert_eq!(f.packets, 64);
        assert_eq!(f.protocol, 6);
        assert_eq!(f.dst_port, 443);
        assert!(
            matches!(f.src_ip, flow_types::IpAddrType::V4(ip) if ip == std::net::Ipv4Addr::new(10, 0, 1, 5))
        );
    }

    #[test]
    fn test_ipfix_enterprise_ie_skipped() {
        let mut pkt = Vec::new();
        let template_id: u16 = 400;
        // Two fields: enterprise field (IE 1 with enterprise bit) + real IN_BYTES
        // Enterprise field: type = 0x8001 (bit 15 set), length = 4, enterprise_num = 4 bytes
        // Real field: IANA_IN_BYTES = 1, length = 8

        // Template set body: template_id, field_count=2, then the two field entries
        // enterprise entry: 4 bytes type+len + 4 bytes enterprise number = 8 bytes consumed in template
        // real entry: 4 bytes
        let tmpl_body_len = 4 + 8 + 4; // template header + enterprise field (with enterprise num) + real field
        let tmpl_set_len = 4 + tmpl_body_len;
        // Data set: 8 bytes (IN_BYTES only, enterprise field skipped in template so record_size=8)
        let data_set_len = 4 + 8;
        let total_len = 16 + tmpl_set_len + data_set_len;

        // IPFIX header
        pkt.extend_from_slice(&10u16.to_be_bytes());
        pkt.extend_from_slice(&(total_len as u16).to_be_bytes());
        pkt.extend_from_slice(&1700000000u32.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&200u32.to_be_bytes());

        // Template set
        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&(tmpl_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&2u16.to_be_bytes()); // field_count = 2
                                                    // Enterprise field (should be skipped)
        pkt.extend_from_slice(&0x8001u16.to_be_bytes()); // enterprise bit set
        pkt.extend_from_slice(&4u16.to_be_bytes()); // length
        pkt.extend_from_slice(&12345u32.to_be_bytes()); // enterprise number
                                                        // Real field
        pkt.extend_from_slice(&IANA_IN_BYTES.to_be_bytes());
        pkt.extend_from_slice(&8u16.to_be_bytes());

        // Data set
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(data_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&9999u64.to_be_bytes()); // bytes value

        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 3));
        let mut cache = ThreadLocalTemplateCache::new();
        let flows = parse_packet(&pkt, &mut cache, exporter).expect("enterprise ie test failed");

        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].bytes, 9999);
    }

    #[test]
    fn test_ipfix_varlen_field_skipped() {
        // Template: varlen field (e.g. applicationName) + IN_BYTES(8)
        let template_id: u16 = 500;
        let tmpl_set_len = 4 + 4 + 4 + 4; // set hdr + tmpl hdr + 2 fields
                                          // Record: varlen prefix (1) + 3 bytes payload + 8 bytes IN_BYTES
        let data_set_len = 4 + 1 + 3 + 8;
        let total_len = 16 + tmpl_set_len + data_set_len;

        let mut pkt = Vec::new();
        pkt.extend_from_slice(&10u16.to_be_bytes());
        pkt.extend_from_slice(&(total_len as u16).to_be_bytes());
        pkt.extend_from_slice(&1700000000u32.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&300u32.to_be_bytes());

        // Template set: field 96 (applicationName) varlen + IN_BYTES fixed 8
        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&(tmpl_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&96u16.to_be_bytes());
        pkt.extend_from_slice(&VARLEN.to_be_bytes());
        pkt.extend_from_slice(&IANA_IN_BYTES.to_be_bytes());
        pkt.extend_from_slice(&8u16.to_be_bytes());

        // Data set: varlen "abc" + bytes=7777
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(data_set_len as u16).to_be_bytes());
        pkt.push(3u8); // varlen prefix
        pkt.extend_from_slice(b"abc");
        pkt.extend_from_slice(&7777u64.to_be_bytes());

        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 4));
        let mut cache = ThreadLocalTemplateCache::new();
        let flows = parse_packet(&pkt, &mut cache, exporter).expect("varlen parse failed");

        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].bytes, 7777);
    }

    #[test]
    fn test_read_uint_oversized_field() {
        // 12-byte field: high 4 bytes must be ignored, low 8 kept
        let mut data = vec![0xFFu8, 0xFF, 0xFF, 0xFF];
        data.extend_from_slice(&42u64.to_be_bytes());
        assert_eq!(read_uint(&data), 42);
    }

    #[test]
    fn test_template_field_count_clamped() {
        // Template claiming 65535 fields inside a 20-byte set must neither
        // over-allocate nor panic
        let template_id: u16 = 600;
        let tmpl_set_len = 4 + 4 + 4; // room for a single field entry
        let total_len = 16 + tmpl_set_len;

        let mut pkt = Vec::new();
        pkt.extend_from_slice(&10u16.to_be_bytes());
        pkt.extend_from_slice(&(total_len as u16).to_be_bytes());
        pkt.extend_from_slice(&1700000000u32.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&400u32.to_be_bytes());

        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&(tmpl_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&65535u16.to_be_bytes()); // absurd field_count
        pkt.extend_from_slice(&IANA_IN_BYTES.to_be_bytes());
        pkt.extend_from_slice(&8u16.to_be_bytes());

        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 5));
        let mut cache = ThreadLocalTemplateCache::new();
        let result = parse_packet(&pkt, &mut cache, exporter);
        assert!(result.is_ok());
    }

    #[test]
    fn test_v9_options_template_sampling() {
        // Packet 1: data template + options template + options data (rate 1000)
        // Packet 2: data record — counters must come back scaled ×1000
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 6));
        let mut cache = ThreadLocalTemplateCache::new();

        // First: regular template + flow via helper (rate not learned yet → ×1)
        let pkt = build_v9_packet([192, 168, 1, 10], [8, 8, 8, 8], 100, 10);
        let flows = parse_packet(&pkt, &mut cache, exporter).expect("v9 parse");
        assert_eq!(flows[0].bytes, 100);
        assert_eq!(flows[0].sampling_rate, 1);

        // Options template (id 700): scope System(1,4) + samplingInterval(34,4)
        let opts_tmpl_id: u16 = 700;
        let mut pkt2 = Vec::new();
        pkt2.extend_from_slice(&9u16.to_be_bytes());
        pkt2.extend_from_slice(&2u16.to_be_bytes());
        pkt2.extend_from_slice(&1000u32.to_be_bytes());
        pkt2.extend_from_slice(&1700000100u32.to_be_bytes());
        pkt2.extend_from_slice(&2u32.to_be_bytes());
        pkt2.extend_from_slice(&100u32.to_be_bytes()); // same source_id as build_v9_packet

        // Options template flowset (id=1):
        // tmpl_id + scope_len(4) + option_len(4) + 1 scope field + 1 option field
        let fs_len = 4 + 6 + 4 + 4;
        pkt2.extend_from_slice(&1u16.to_be_bytes());
        pkt2.extend_from_slice(&(fs_len as u16).to_be_bytes());
        pkt2.extend_from_slice(&opts_tmpl_id.to_be_bytes());
        pkt2.extend_from_slice(&4u16.to_be_bytes()); // scope length (bytes)
        pkt2.extend_from_slice(&4u16.to_be_bytes()); // option length (bytes)
        pkt2.extend_from_slice(&1u16.to_be_bytes()); // scope: System
        pkt2.extend_from_slice(&4u16.to_be_bytes());
        pkt2.extend_from_slice(&IANA_SAMPLING_INTERVAL.to_be_bytes());
        pkt2.extend_from_slice(&4u16.to_be_bytes());

        // Options data flowset (id=700): scope(4) + rate 1000 (4)
        let od_len = 4 + 4 + 4;
        pkt2.extend_from_slice(&opts_tmpl_id.to_be_bytes());
        pkt2.extend_from_slice(&(od_len as u16).to_be_bytes());
        pkt2.extend_from_slice(&0u32.to_be_bytes()); // scope value
        pkt2.extend_from_slice(&1000u32.to_be_bytes()); // samplingInterval

        let flows = parse_packet(&pkt2, &mut cache, exporter).expect("v9 options parse");
        assert!(flows.is_empty()); // options data produces no flows

        // Now a data record: counters scaled ×1000
        let pkt3 = build_v9_packet([192, 168, 1, 10], [8, 8, 8, 8], 100, 10);
        let flows = parse_packet(&pkt3, &mut cache, exporter).expect("v9 scaled parse");
        assert_eq!(flows[0].bytes, 100_000);
        assert_eq!(flows[0].packets, 10_000);
        assert_eq!(flows[0].sampling_rate, 1000);
    }

    #[test]
    fn test_ipfix_options_template_sampling() {
        // samplingPacketInterval=1, samplingPacketSpace=999 → rate 1000
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 7));
        let mut cache = ThreadLocalTemplateCache::new();

        let opts_tmpl_id: u16 = 800;
        // Options template set (id=3): tmpl_id + field_count=3 + scope_count=1,
        // fields: scope obsDomain(149,4), pktInterval(305,4), pktSpace(306,4)
        let ots_len = 4 + 6 + 3 * 4;
        // Options data set: scope(4) + interval(4) + space(4)
        let od_len = 4 + 12;
        let total = 16 + ots_len + od_len;

        let mut pkt = Vec::new();
        pkt.extend_from_slice(&10u16.to_be_bytes());
        pkt.extend_from_slice(&(total as u16).to_be_bytes());
        pkt.extend_from_slice(&1700000000u32.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&100u32.to_be_bytes());

        pkt.extend_from_slice(&3u16.to_be_bytes());
        pkt.extend_from_slice(&(ots_len as u16).to_be_bytes());
        pkt.extend_from_slice(&opts_tmpl_id.to_be_bytes());
        pkt.extend_from_slice(&3u16.to_be_bytes()); // field_count
        pkt.extend_from_slice(&1u16.to_be_bytes()); // scope_field_count
        pkt.extend_from_slice(&149u16.to_be_bytes()); // scope: observationDomainId
        pkt.extend_from_slice(&4u16.to_be_bytes());
        pkt.extend_from_slice(&IANA_SAMPLING_PKT_INTERVAL.to_be_bytes());
        pkt.extend_from_slice(&4u16.to_be_bytes());
        pkt.extend_from_slice(&IANA_SAMPLING_PKT_SPACE.to_be_bytes());
        pkt.extend_from_slice(&4u16.to_be_bytes());

        pkt.extend_from_slice(&opts_tmpl_id.to_be_bytes());
        pkt.extend_from_slice(&(od_len as u16).to_be_bytes());
        pkt.extend_from_slice(&100u32.to_be_bytes()); // scope value
        pkt.extend_from_slice(&1u32.to_be_bytes()); // interval
        pkt.extend_from_slice(&999u32.to_be_bytes()); // space

        let flows = parse_packet(&pkt, &mut cache, exporter).expect("ipfix options parse");
        assert!(flows.is_empty());

        // Regular IPFIX flow from the same observation domain gets scaled
        let pkt2 = build_ipfix_packet([10, 0, 1, 5], [1, 1, 1, 1], 50, 5);
        let flows = parse_packet(&pkt2, &mut cache, exporter).expect("ipfix scaled parse");
        assert_eq!(flows[0].bytes, 50_000);
        assert_eq!(flows[0].packets, 5_000);
        assert_eq!(flows[0].sampling_rate, 1000);
    }

    #[test]
    fn test_flow_direction_decoded() {
        // Template: flowDirection(61,1) + ingressIface(10,4) + IN_BYTES(8)
        let template_id: u16 = 900;
        let tmpl_set_len = 4 + 4 + 3 * 4;
        let data_set_len = 4 + 1 + 4 + 8;
        let total = 16 + tmpl_set_len + data_set_len;

        let mut pkt = Vec::new();
        pkt.extend_from_slice(&10u16.to_be_bytes());
        pkt.extend_from_slice(&(total as u16).to_be_bytes());
        pkt.extend_from_slice(&1700000000u32.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&500u32.to_be_bytes());

        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&(tmpl_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&3u16.to_be_bytes());
        pkt.extend_from_slice(&IANA_FLOW_DIRECTION.to_be_bytes());
        pkt.extend_from_slice(&1u16.to_be_bytes());
        pkt.extend_from_slice(&IANA_INGRESS_IFACE.to_be_bytes());
        pkt.extend_from_slice(&4u16.to_be_bytes());
        pkt.extend_from_slice(&IANA_IN_BYTES.to_be_bytes());
        pkt.extend_from_slice(&8u16.to_be_bytes());

        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(data_set_len as u16).to_be_bytes());
        pkt.push(1u8); // egress
        pkt.extend_from_slice(&7u32.to_be_bytes()); // ingress iface 7
        pkt.extend_from_slice(&1234u64.to_be_bytes());

        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 8));
        let mut cache = ThreadLocalTemplateCache::new();
        let flows = parse_packet(&pkt, &mut cache, exporter).expect("direction parse");

        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].direction, 1);
        assert_eq!(flows[0].ingress_interface, 7);
        assert_eq!(flows[0].bytes, 1234);
    }

    #[test]
    fn test_flow_direction_absent_is_unknown() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 9));
        let pkt = build_v9_packet([192, 168, 1, 10], [8, 8, 8, 8], 100, 10);
        let flows = parse_packet(&pkt, &mut cache, exporter).expect("v9 parse");
        assert_eq!(flows[0].direction, flow_types::DIRECTION_UNKNOWN);
    }

    #[test]
    fn test_unsupported_version() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        let pkt = [0x00, 0x05u8, 0, 0, 0, 0]; // version 5
        let result = parse_packet(&pkt, &mut cache, exporter);
        assert!(matches!(result, Err(ParseError::UnsupportedVersion(5))));
    }

    // Builds a v9 packet whose template carries FIRST/LAST_SWITCHED
    fn build_v9_switched_packet(sys_uptime: u32, first: u32, last: u32) -> Vec<u8> {
        let mut pkt = Vec::new();
        pkt.extend_from_slice(&9u16.to_be_bytes());
        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&sys_uptime.to_be_bytes());
        pkt.extend_from_slice(&1_700_000_000u32.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&100u32.to_be_bytes());

        let fields: &[(u16, u16)] = &[
            (IANA_FIRST_SWITCHED, 4),
            (IANA_LAST_SWITCHED, 4),
            (IANA_IN_BYTES, 8),
        ];
        let template_id: u16 = 320;
        let fs_len = 4 + 4 + fields.len() * 4;
        pkt.extend_from_slice(&0u16.to_be_bytes());
        pkt.extend_from_slice(&(fs_len as u16).to_be_bytes());
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(fields.len() as u16).to_be_bytes());
        for (ft, fl) in fields {
            pkt.extend_from_slice(&ft.to_be_bytes());
            pkt.extend_from_slice(&fl.to_be_bytes());
        }

        let data_len = 4 + 4 + 4 + 8;
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(data_len as u16).to_be_bytes());
        pkt.extend_from_slice(&first.to_be_bytes());
        pkt.extend_from_slice(&last.to_be_bytes());
        pkt.extend_from_slice(&1234u64.to_be_bytes());
        pkt
    }

    #[test]
    fn test_v9_first_last_switched() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        // uptime 100s; flow ran from uptime 40s to 90s
        let pkt = build_v9_switched_packet(100_000, 40_000, 90_000);
        let flows = parse_packet(&pkt, &mut cache, exporter).unwrap();
        assert_eq!(flows.len(), 1);
        let export_ms = 1_700_000_000u64 * 1000;
        assert_eq!(flows[0].start_ms, export_ms - 60_000);
        assert_eq!(flows[0].end_ms, export_ms - 10_000);
    }

    #[test]
    fn test_v9_switched_uptime_wrap() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        // uptime wrapped: header says 5000ms, flow started 4096ms before the wrap
        let pkt = build_v9_switched_packet(5_000, 0xFFFF_F000, 2_000);
        let flows = parse_packet(&pkt, &mut cache, exporter).unwrap();
        let export_ms = 1_700_000_000u64 * 1000;
        // age = 5000 - (-4096) = 9096ms
        assert_eq!(flows[0].start_ms, export_ms - 9_096);
        assert_eq!(flows[0].end_ms, export_ms - 3_000);
    }

    #[test]
    fn test_ipfix_flow_millis() {
        let mut pkt = Vec::new();
        let template_id: u16 = 420;
        let fields: &[(u16, u16)] = &[
            (IANA_FLOW_START_MS, 8),
            (IANA_FLOW_END_MS, 8),
            (IANA_IN_BYTES, 8),
        ];
        let tmpl_set_len = 4 + 4 + fields.len() * 4;
        let data_set_len = 4 + 24;
        let total_len = 16 + tmpl_set_len + data_set_len;

        pkt.extend_from_slice(&10u16.to_be_bytes());
        pkt.extend_from_slice(&(total_len as u16).to_be_bytes());
        pkt.extend_from_slice(&1_700_000_000u32.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&100u32.to_be_bytes());

        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&(tmpl_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(fields.len() as u16).to_be_bytes());
        for (ft, fl) in fields {
            pkt.extend_from_slice(&ft.to_be_bytes());
            pkt.extend_from_slice(&fl.to_be_bytes());
        }

        let start: u64 = 1_699_999_980_500;
        let end: u64 = 1_699_999_995_250;
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(data_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&start.to_be_bytes());
        pkt.extend_from_slice(&end.to_be_bytes());
        pkt.extend_from_slice(&4096u64.to_be_bytes());

        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 2));
        let flows = parse_packet(&pkt, &mut cache, exporter).unwrap();
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].start_ms, start);
        assert_eq!(flows[0].end_ms, end);
    }

    #[test]
    fn test_flow_without_timestamps_stays_zero() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        let pkt = build_v9_packet([192, 168, 1, 10], [8, 8, 8, 8], 4096, 32);
        let flows = parse_packet(&pkt, &mut cache, exporter).unwrap();
        assert_eq!(flows[0].start_ms, 0);
        assert_eq!(flows[0].end_ms, 0);
    }

    // IPFIX com IE 21/22 (sysUptime-relativos) — sem IE 160 o parser ancora o
    // fim no export_time preservando a duração exata
    fn build_ipfix_switched_packet(first: u32, last: u32, with_init: Option<u64>) -> Vec<u8> {
        let mut pkt = Vec::new();
        let template_id: u16 = 500;
        let fields: &[(u16, u16)] = &[
            (IANA_FIRST_SWITCHED, 4),
            (IANA_LAST_SWITCHED, 4),
            (IANA_IN_BYTES, 8),
        ];
        let tmpl_set_len = 4 + 4 + fields.len() * 4;
        let data_set_len = 4 + 16;

        // Options: scope exportingProcessId(130,4) + systemInitTimeMilliseconds(160,8)
        let (opt_tmpl_len, opt_data_len) = if with_init.is_some() {
            (4 + 6 + 8, 4 + 12)
        } else {
            (0, 0)
        };
        let total_len = 16 + tmpl_set_len + opt_tmpl_len + opt_data_len + data_set_len;

        pkt.extend_from_slice(&10u16.to_be_bytes());
        pkt.extend_from_slice(&(total_len as u16).to_be_bytes());
        pkt.extend_from_slice(&1_700_000_000u32.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&100u32.to_be_bytes());

        pkt.extend_from_slice(&2u16.to_be_bytes());
        pkt.extend_from_slice(&(tmpl_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(fields.len() as u16).to_be_bytes());
        for (ft, fl) in fields {
            pkt.extend_from_slice(&ft.to_be_bytes());
            pkt.extend_from_slice(&fl.to_be_bytes());
        }

        if let Some(init) = with_init {
            let opt_id: u16 = 501;
            pkt.extend_from_slice(&3u16.to_be_bytes());
            pkt.extend_from_slice(&(opt_tmpl_len as u16).to_be_bytes());
            pkt.extend_from_slice(&opt_id.to_be_bytes());
            pkt.extend_from_slice(&2u16.to_be_bytes()); // field_count
            pkt.extend_from_slice(&1u16.to_be_bytes()); // scope_field_count
            pkt.extend_from_slice(&130u16.to_be_bytes());
            pkt.extend_from_slice(&4u16.to_be_bytes());
            pkt.extend_from_slice(&IANA_SYSTEM_INIT_TIME_MS.to_be_bytes());
            pkt.extend_from_slice(&8u16.to_be_bytes());

            pkt.extend_from_slice(&opt_id.to_be_bytes());
            pkt.extend_from_slice(&(opt_data_len as u16).to_be_bytes());
            pkt.extend_from_slice(&7u32.to_be_bytes()); // scope value
            pkt.extend_from_slice(&init.to_be_bytes());
        }

        pkt.extend_from_slice(&template_id.to_be_bytes());
        pkt.extend_from_slice(&(data_set_len as u16).to_be_bytes());
        pkt.extend_from_slice(&first.to_be_bytes());
        pkt.extend_from_slice(&last.to_be_bytes());
        pkt.extend_from_slice(&2048u64.to_be_bytes());
        pkt
    }

    #[test]
    fn test_ipfix_switched_fallback_duration() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 9));
        // flow durou 15s (uptime 40s → 55s), sem IE 160
        let pkt = build_ipfix_switched_packet(40_000, 55_000, None);
        let flows = parse_packet(&pkt, &mut cache, exporter).unwrap();
        assert_eq!(flows.len(), 1);
        let export_ms = 1_700_000_000u64 * 1000;
        assert_eq!(flows[0].end_ms, export_ms);
        assert_eq!(flows[0].start_ms, export_ms - 15_000);
    }

    #[test]
    fn test_ipfix_switched_with_sys_init() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 9));
        let boot_ms: u64 = 1_699_999_000_000;
        let pkt = build_ipfix_switched_packet(40_000, 55_000, Some(boot_ms));
        let flows = parse_packet(&pkt, &mut cache, exporter).unwrap();
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].start_ms, boot_ms + 40_000);
        assert_eq!(flows[0].end_ms, boot_ms + 55_000);
    }

    /// Template + one data record with arbitrary (field_id, bytes) pairs
    fn build_packet_with_fields(version: u16, fields: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let template_id: u16 = 310;
        let mut tmpl = Vec::new();
        tmpl.extend_from_slice(&template_id.to_be_bytes());
        tmpl.extend_from_slice(&(fields.len() as u16).to_be_bytes());
        let mut record = Vec::new();
        for (id, data) in fields {
            tmpl.extend_from_slice(&id.to_be_bytes());
            tmpl.extend_from_slice(&(data.len() as u16).to_be_bytes());
            record.extend_from_slice(data);
        }
        let tmpl_set_id: u16 = if version == 9 { 0 } else { 2 };
        let mut sets = Vec::new();
        sets.extend_from_slice(&tmpl_set_id.to_be_bytes());
        sets.extend_from_slice(&((4 + tmpl.len()) as u16).to_be_bytes());
        sets.extend_from_slice(&tmpl);
        sets.extend_from_slice(&template_id.to_be_bytes());
        sets.extend_from_slice(&((4 + record.len()) as u16).to_be_bytes());
        sets.extend_from_slice(&record);

        let mut pkt = Vec::new();
        if version == 9 {
            pkt.extend_from_slice(&9u16.to_be_bytes());
            pkt.extend_from_slice(&2u16.to_be_bytes());
            pkt.extend_from_slice(&1000u32.to_be_bytes());
            pkt.extend_from_slice(&1700000000u32.to_be_bytes());
            pkt.extend_from_slice(&1u32.to_be_bytes());
            pkt.extend_from_slice(&100u32.to_be_bytes());
        } else {
            pkt.extend_from_slice(&10u16.to_be_bytes());
            pkt.extend_from_slice(&((16 + sets.len()) as u16).to_be_bytes());
            pkt.extend_from_slice(&1700000000u32.to_be_bytes());
            pkt.extend_from_slice(&1u32.to_be_bytes());
            pkt.extend_from_slice(&100u32.to_be_bytes());
        }
        pkt.extend_from_slice(&sets);
        pkt
    }

    fn base_fields() -> Vec<(u16, Vec<u8>)> {
        vec![
            (IANA_IN_BYTES, 1000u64.to_be_bytes().to_vec()),
            (IANA_IN_PKTS, 10u64.to_be_bytes().to_vec()),
            (IANA_PROTOCOL, vec![6]),
            (IANA_IPV4_SRC_ADDR, vec![100, 68, 29, 12]),
            (IANA_L4_SRC_PORT, 50000u16.to_be_bytes().to_vec()),
            (IANA_IPV4_DST_ADDR, vec![170, 231, 6, 78]),
            (IANA_L4_DST_PORT, 22u16.to_be_bytes().to_vec()),
        ]
    }

    fn v4(a: u8, b: u8, c: u8, d: u8) -> Option<flow_types::IpAddrType> {
        Some(flow_types::IpAddrType::V4(std::net::Ipv4Addr::new(
            a, b, c, d,
        )))
    }

    // AC-01 (task 17.8)
    #[test]
    fn test_ipfix_post_nat_fields() {
        let mut fields = base_fields();
        fields.push((IANA_POST_NAT_SRC_IPV4, vec![170, 231, 6, 78]));
        fields.push((IANA_POST_NAT_DST_IPV4, vec![100, 68, 40, 7]));
        fields.push((IANA_POST_NAPT_SRC_PORT, 61000u16.to_be_bytes().to_vec()));
        fields.push((IANA_POST_NAPT_DST_PORT, 22u16.to_be_bytes().to_vec()));
        let pkt = build_packet_with_fields(10, &fields);

        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        let flows = parse_packet(&pkt, &mut cache, exporter).expect("ipfix nat parse");
        assert_eq!(flows.len(), 1);
        let f = &flows[0];
        assert_eq!(f.nat_src_ip, v4(170, 231, 6, 78));
        assert_eq!(f.nat_dst_ip, v4(100, 68, 40, 7));
        assert_eq!(f.nat_src_port, 61000);
        assert_eq!(f.nat_dst_port, 22);
        assert_eq!(f.dst_port, 22);
    }

    // AC-02 (task 17.8): v9 keeps IDs above 32767 unmasked
    #[test]
    fn test_v9_nsel_xlate_fields() {
        let mut fields = base_fields();
        fields.push((NSEL_XLATE_SRC_ADDR_IPV4, vec![170, 231, 6, 78]));
        fields.push((NSEL_XLATE_DST_ADDR_IPV4, vec![100, 68, 40, 7]));
        fields.push((NSEL_XLATE_SRC_PORT, 61000u16.to_be_bytes().to_vec()));
        fields.push((NSEL_XLATE_DST_PORT, 2222u16.to_be_bytes().to_vec()));
        // 40001 & 0x7FFF = 7233: must not be read as anything else
        let pkt = build_packet_with_fields(9, &fields);

        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        let flows = parse_packet(&pkt, &mut cache, exporter).expect("v9 nsel parse");
        assert_eq!(flows.len(), 1);
        let f = &flows[0];
        assert_eq!(f.nat_src_ip, v4(170, 231, 6, 78));
        assert_eq!(f.nat_dst_ip, v4(100, 68, 40, 7));
        assert_eq!(f.nat_src_port, 61000);
        assert_eq!(f.nat_dst_port, 2222);
        assert_eq!(f.bytes, 1000);
    }

    // AC-03 (task 17.8)
    #[test]
    fn test_flow_without_nat_has_empty_nat_fields() {
        let pkt = build_packet_with_fields(10, &base_fields());
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        let f = parse_packet(&pkt, &mut cache, exporter).expect("parse")[0];
        assert_eq!((f.nat_src_ip, f.nat_dst_ip), (None, None));
        assert_eq!((f.nat_src_port, f.nat_dst_port), (0, 0));
    }
}
