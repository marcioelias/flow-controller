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

/// Header for Netflow v9
#[derive(Debug, Clone, Copy)]
pub struct V9Header {
    pub version: u16,
    pub count: u16,
    pub sys_uptime: u32,
    pub unix_secs: u32,
    pub seq_num: u32,
    pub source_id: u32,
}

/// Parses the Netflow V9 Header specifically.
/// Uses a simple Cursor over the slice for safety and zero-allocation.
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

/// Parses the overall payload. 
/// In a full implementation, this will iterate through the data sets and template sets,
/// querying the template cache directly and returning a pre-allocated Vec (or Iterator) of NormalizedFlow.
pub const IANA_IN_BYTES: u16 = 1;
pub const IANA_IN_PKTS: u16 = 2;
pub const IANA_PROTOCOL: u16 = 4;
pub const IANA_TCP_FLAGS: u16 = 6;
pub const IANA_L4_SRC_PORT: u16 = 7;
pub const IANA_IPV4_SRC_ADDR: u16 = 8;
pub const IANA_L4_DST_PORT: u16 = 11;
pub const IANA_IPV4_DST_ADDR: u16 = 12;

#[inline]
fn read_uint(bytes: &[u8]) -> u64 {
    let mut res = 0;
    for &b in bytes {
        res = (res << 8) | (b as u64);
    }
    res
}

pub fn parse_packet(payload: &[u8], templates: &mut ThreadLocalTemplateCache, exporter_ip: std::net::IpAddr) -> Result<Vec<NormalizedFlow>, ParseError> {
    let mut cur = Cursor::new(payload);
    let mut flows = Vec::with_capacity(32); // Preallocate for common frame size

    match cur.read_u16::<BigEndian>() {
        Ok(9) => {
            cur.set_position(0);
            let hdr = parse_v9_header(&mut cur)?;
            
            let mut ptr = 20; // V9 header size
            while ptr + 4 <= payload.len() {
                let mut iter_cur = Cursor::new(&payload[ptr..ptr+4]);
                let flowset_id = iter_cur.read_u16::<BigEndian>()?;
                let length = iter_cur.read_u16::<BigEndian>()? as usize;
                
                if length < 4 { break; } // Prevent infinite loop or invalid len
                if ptr + length > payload.len() { break; } // Incomplete
                
                match flowset_id {
                    0 => {
                        // Parse Template Records
                        let mut t_ptr = ptr + 4;
                        while t_ptr + 4 <= ptr + length {
                            let mut t_cur = Cursor::new(&payload[t_ptr..t_ptr+4]);
                            let template_id = t_cur.read_u16::<BigEndian>()?;
                            let field_count = t_cur.read_u16::<BigEndian>()?;
                            t_ptr += 4;
                            
                            let template_len = (field_count as usize) * 4;
                            if t_ptr + template_len > ptr + length { break; }
                            
                            let mut fields = Vec::with_capacity(field_count as usize);
                            let mut f_cur = Cursor::new(&payload[t_ptr..t_ptr+template_len]);
                            for _ in 0..field_count {
                                fields.push(template_cache::TemplateField {
                                    field_type: f_cur.read_u16::<BigEndian>().unwrap_or(0),
                                    length: f_cur.read_u16::<BigEndian>().unwrap_or(0),
                                });
                            }
                            t_ptr += template_len;
                            
                            let ipv4_exporter = match exporter_ip {
                                std::net::IpAddr::V4(v4) => v4,
                                std::net::IpAddr::V6(_) => std::net::Ipv4Addr::new(0,0,0,0),
                            };
                            
                            templates.insert(template_cache::Template {
                                key: template_cache::TemplateKey {
                                    exporter_ip: ipv4_exporter,
                                    source_id: hdr.source_id,
                                    template_id,
                                },
                                fields,
                                timestamp: hdr.unix_secs as u64,
                            });
                        }
                    },
                    1 => {
                        // Options Template, ignore for now
                    },
                    id if id > 255 => {
                        // Parse Data Records using template `id` from cache
                        let ipv4_exporter = match exporter_ip {
                            std::net::IpAddr::V4(v4) => v4,
                            std::net::IpAddr::V6(_) => std::net::Ipv4Addr::new(0,0,0,0),
                        };
                        let key = template_cache::TemplateKey {
                            exporter_ip: ipv4_exporter,
                            source_id: hdr.source_id,
                            template_id: id,
                        };
                        
                        if let Some(template) = templates.get(&key) {
                            let mut d_ptr = ptr + 4;
                            let record_size: usize = template.fields.iter().map(|f| f.length as usize).sum();
                            
                            if record_size > 0 {
                                while d_ptr + record_size <= ptr + length {
                                    let mut flow = NormalizedFlow {
                                        timestamp: hdr.unix_secs as u64,
                                        exporter_ip: ipv4_exporter,
                                        src_ip: flow_types::IpAddrType::V4(std::net::Ipv4Addr::new(0,0,0,0)),
                                        dst_ip: flow_types::IpAddrType::V4(std::net::Ipv4Addr::new(0,0,0,0)),
                                        src_port: 0, dst_port: 0, protocol: 0, bytes: 0, packets: 0,
                                        src_asn: 0, dst_asn: 0, ingress_interface: 0, egress_interface: 0, tcp_flags: 0,
                                    };
                                    
                                    for field in &template.fields {
                                        let f_len = field.length as usize;
                                        if d_ptr + f_len > ptr + length { break; }
                                        let field_data = &payload[d_ptr..d_ptr+f_len];
                                        
                                        match field.field_type {
                                            IANA_IN_BYTES => flow.bytes = read_uint(field_data),
                                            IANA_IN_PKTS => flow.packets = read_uint(field_data),
                                            IANA_PROTOCOL => if f_len == 1 { flow.protocol = field_data[0] },
                                            IANA_TCP_FLAGS => if f_len > 0 { flow.tcp_flags = field_data[f_len - 1] },
                                            IANA_L4_SRC_PORT => flow.src_port = read_uint(field_data) as u16,
                                            IANA_L4_DST_PORT => flow.dst_port = read_uint(field_data) as u16,
                                            IANA_IPV4_SRC_ADDR => if f_len == 4 {
                                                let mut arr = [0;4]; arr.copy_from_slice(field_data);
                                                flow.src_ip = flow_types::IpAddrType::V4(std::net::Ipv4Addr::from(arr));
                                            },
                                            IANA_IPV4_DST_ADDR => if f_len == 4 {
                                                let mut arr = [0;4]; arr.copy_from_slice(field_data);
                                                flow.dst_ip = flow_types::IpAddrType::V4(std::net::Ipv4Addr::from(arr));
                                            },
                                            _ => {}
                                        }
                                        d_ptr += f_len;
                                    }
                                    flows.push(flow);
                                }
                            }
                        }
                    },
                    _ => {}
                }
                
                ptr += length;
            }
        },
        Ok(10) => {
            // IPFIX (v10) - Structurally very similar to v9, but with different Header.
            // Keeping empty for now as requested.
        },
        Ok(v) => return Err(ParseError::UnsupportedVersion(v)),
        Err(e) => return Err(ParseError::Io(e)),
    }
    Ok(flows)
}
