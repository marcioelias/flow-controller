use byteorder::{BigEndian, ReadBytesExt};
use flow_types::NormalizedFlow;
use thiserror::Error;
use std::io::Cursor;
use template_cache::ThreadLocalTemplateCache;

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
pub const IANA_L4_DST_PORT: u16 = 11;
pub const IANA_IPV4_DST_ADDR: u16 = 12;
pub const IANA_BGP_SRC_ASN: u16 = 16;
pub const IANA_BGP_DST_ASN: u16 = 17;
pub const IANA_IPV6_SRC_ADDR: u16 = 27;
pub const IANA_IPV6_DST_ADDR: u16 = 28;

#[inline]
fn read_uint(bytes: &[u8]) -> u64 {
    let mut res = 0u64;
    for &b in bytes {
        res = (res << 8) | (b as u64);
    }
    res
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

        let mut fields = Vec::with_capacity(field_count);
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

            fields.push(template_cache::TemplateField {
                field_type: raw_type & 0x7FFF, // clear enterprise bit just in case
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
                timestamp: export_time as u64,
            });
        }
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

    let record_size: usize = template.fields.iter().map(|f| f.length as usize).sum();
    if record_size == 0 {
        return vec![];
    }

    let mut flows = Vec::new();
    let mut d_ptr = 0usize;

    while d_ptr + record_size <= set_data.len() {
        let mut flow = NormalizedFlow {
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
        };

        let mut f_ptr = d_ptr;
        for field in &template.fields {
            let f_len = field.length as usize;
            if f_ptr + f_len > set_data.len() {
                break;
            }
            let field_data = &set_data[f_ptr..f_ptr + f_len];

            match field.field_type {
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
                _ => {}
            }
            f_ptr += f_len;
        }

        flows.push(flow);
        d_ptr += record_size;
    }

    flows
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
                parse_template_set(set_data, hdr.source_id, hdr.unix_secs, ipv4_exporter, templates, false);
            }
            1 => { /* Options Template — skip */ }
            id if id > 255 => {
                let mut data_flows =
                    parse_data_set(set_data, id, hdr.source_id, hdr.unix_secs, ipv4_exporter, templates);
                flows.append(&mut data_flows);
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
            3 => { /* Options Template Set — skip */ }
            id if id >= 256 => {
                let mut data_flows = parse_data_set(
                    set_data,
                    id,
                    hdr.observation_domain_id,
                    hdr.export_time,
                    ipv4_exporter,
                    templates,
                );
                flows.append(&mut data_flows);
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
        pkt.extend_from_slice(&1u32.to_be_bytes());          // seq_num
        pkt.extend_from_slice(&100u32.to_be_bytes());         // observation_domain_id

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
        assert!(matches!(f.src_ip, flow_types::IpAddrType::V4(ip) if ip == std::net::Ipv4Addr::new(192, 168, 1, 10)));
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
        assert!(matches!(f.src_ip, flow_types::IpAddrType::V4(ip) if ip == std::net::Ipv4Addr::new(10, 0, 1, 5)));
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
        pkt.extend_from_slice(&4u16.to_be_bytes());      // length
        pkt.extend_from_slice(&12345u32.to_be_bytes());  // enterprise number
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
    fn test_unsupported_version() {
        let mut cache = ThreadLocalTemplateCache::new();
        let exporter = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        let pkt = [0x00, 0x05u8, 0, 0, 0, 0]; // version 5
        let result = parse_packet(&pkt, &mut cache, exporter);
        assert!(matches!(result, Err(ParseError::UnsupportedVersion(5))));
    }
}
