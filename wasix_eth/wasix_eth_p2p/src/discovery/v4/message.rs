use alloy_rlp::{Encodable, Decodable, Header, BufMut};
use alloy_primitives::{B256, B512};
use base64::Engine;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;
use k256::ecdsa::signature::hazmat::PrehashSigner;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeEndpoint {
    pub ip: IpAddr,
    pub udp_port: u16,
    pub tcp_port: u16,
}

impl Encodable for NodeEndpoint {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let h = Header { list: true, payload_length: self.payload_length() };
        h.encode(out);
        match self.ip {
            IpAddr::V4(ip) => {
                ip.octets().as_slice().encode(out);
            }
            IpAddr::V6(ip) => {
                ip.octets().as_slice().encode(out);
            }
        }
        self.udp_port.encode(out);
        self.tcp_port.encode(out);
    }

    fn length(&self) -> usize {
        let len = self.payload_length();
        alloy_rlp::length_of_length(len) + len
    }
}

impl NodeEndpoint {
    fn payload_length(&self) -> usize {
        let ip_len = match self.ip {
            IpAddr::V4(ip) => ip.octets().as_slice().length(),
            IpAddr::V6(ip) => ip.octets().as_slice().length(),
        };
        ip_len + self.udp_port.length() + self.tcp_port.length()
    }
}

impl FromStr for NodeEndpoint {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let addr = s.parse::<SocketAddr>().map_err(|e| e.to_string())?;
        Ok(NodeEndpoint {
            ip: addr.ip(),
            udp_port: addr.port(),
            tcp_port: addr.port(),
        })
    }
}

pub struct Enode {
    pub id: B512,
    pub ip: IpAddr,
    pub udp_port: u16,
    pub tcp_port: u16,
}

impl FromStr for Enode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with("enode://") {
            return Err("Invalid enode prefix".to_string());
        }
        let rest = &s[8..];
        let parts: Vec<&str> = rest.split('@').collect();
        if parts.len() != 2 {
            return Err("Missing @ in enode".to_string());
        }
        let id_hex = parts[0];
        let id = B512::from_str(id_hex).map_err(|e| e.to_string())?;

        let addr_parts: Vec<&str> = parts[1].split('?').collect();
        let host_port = addr_parts[0];
        
        let last_colon = host_port.rfind(':').ok_or("Missing port in enode")?;
        let host = &host_port[..last_colon];
        let port_str = &host_port[last_colon + 1..];
        
        let ip = host.parse::<IpAddr>().map_err(|e| e.to_string())?;
        let tcp_port = port_str.parse::<u16>().map_err(|e| e.to_string())?;
        let mut udp_port = tcp_port;

        if addr_parts.len() > 1 {
            for param in addr_parts[1].split('&') {
                if let Some(val) = param.strip_prefix("discport=") {
                    udp_port = val.parse::<u16>().map_err(|e| e.to_string())?;
                }
            }
        }

        Ok(Enode { id, ip, udp_port, tcp_port })
    }
}

impl Decodable for NodeEndpoint {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let h = Header::decode(buf)?;
        if !h.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let mut list_slice = &buf[..h.payload_length];
        *buf = &buf[h.payload_length..];

        // Decode IP as RLP string
        let ip_header = Header::decode(&mut list_slice)?;
        if ip_header.list {
            return Err(alloy_rlp::Error::UnexpectedList);
        }
        let ip_bytes = &list_slice[..ip_header.payload_length];
        list_slice = &list_slice[ip_header.payload_length..];

        let ip = if ip_bytes.len() == 4 {
            let mut octets = [0u8; 4];
            octets.copy_from_slice(ip_bytes);
            IpAddr::V4(Ipv4Addr::from(octets))
        } else if ip_bytes.len() == 16 {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(ip_bytes);
            IpAddr::V6(Ipv6Addr::from(octets))
        } else {
            return Err(alloy_rlp::Error::Custom("Invalid IP address length"));
        };
        let udp_port: u16 = Decodable::decode(&mut list_slice)?;
        let tcp_port: u16 = Decodable::decode(&mut list_slice)?;
        Ok(NodeEndpoint { ip, udp_port, tcp_port })
    }
}

#[derive(Clone, Debug)]
pub struct Ping {
    pub version: u32,
    pub from: NodeEndpoint,
    pub to: NodeEndpoint,
    pub expiration: u64,
    pub enr_seq: Option<u64>,
}

impl Encodable for Ping {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let mut payload_len = self.version.length() + self.from.length() + self.to.length() + self.expiration.length();
        if let Some(seq) = self.enr_seq {
            payload_len += seq.length();
        }
        let h = Header { list: true, payload_length: payload_len };
        h.encode(out);
        self.version.encode(out);
        self.from.encode(out);
        self.to.encode(out);
        self.expiration.encode(out);
        if let Some(seq) = self.enr_seq {
            seq.encode(out);
        }
    }

    fn length(&self) -> usize {
        let mut payload_len = self.version.length() + self.from.length() + self.to.length() + self.expiration.length();
        if let Some(seq) = self.enr_seq {
            payload_len += seq.length();
        }
        alloy_rlp::length_of_length(payload_len) + payload_len
    }
}

impl Decodable for Ping {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let mut list_slice = &buf[..header.payload_length];
        *buf = &buf[header.payload_length..];

        let version = Decodable::decode(&mut list_slice)?;
        let from = Decodable::decode(&mut list_slice)?;
        let to = Decodable::decode(&mut list_slice)?;
        let expiration = Decodable::decode(&mut list_slice)?;
        
        let enr_seq = if !list_slice.is_empty() {
            Some(Decodable::decode(&mut list_slice)?)
        } else {
            None
        };

        Ok(Self { version, from, to, expiration, enr_seq })
    }
}

#[derive(Clone, Debug)]
pub struct Pong {
    pub to: NodeEndpoint,
    pub echo: B256,
    pub expiration: u64,
    pub enr_seq: Option<u64>,
}

impl Encodable for Pong {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let mut payload_len = self.to.length() + self.echo.length() + self.expiration.length();
        if let Some(seq) = self.enr_seq {
            payload_len += seq.length();
        }
        let h = Header { list: true, payload_length: payload_len };
        h.encode(out);
        self.to.encode(out);
        self.echo.encode(out);
        self.expiration.encode(out);
        if let Some(seq) = self.enr_seq {
            seq.encode(out);
        }
    }

    fn length(&self) -> usize {
        let mut payload_len = self.to.length() + self.echo.length() + self.expiration.length();
        if let Some(seq) = self.enr_seq {
            payload_len += seq.length();
        }
        alloy_rlp::length_of_length(payload_len) + payload_len
    }
}

impl Decodable for Pong {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let mut list_slice = &buf[..header.payload_length];
        *buf = &buf[header.payload_length..];

        let to = Decodable::decode(&mut list_slice)?;
        let echo = Decodable::decode(&mut list_slice)?;
        let expiration = Decodable::decode(&mut list_slice)?;
        
        let enr_seq = if !list_slice.is_empty() {
            Some(Decodable::decode(&mut list_slice)?)
        } else {
            None
        };

        Ok(Self { to, echo, expiration, enr_seq })
    }
}

#[derive(Clone, Debug)]
pub struct FindNode {
    pub target: B512,
    pub expiration: u64,
}

impl Encodable for FindNode {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let h = Header { list: true, payload_length: self.target.length() + self.expiration.length() };
        h.encode(out);
        self.target.encode(out);
        self.expiration.encode(out);
    }

    fn length(&self) -> usize {
        let payload_len = self.target.length() + self.expiration.length();
        alloy_rlp::length_of_length(payload_len) + payload_len
    }
}

impl Decodable for FindNode {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let mut list_slice = &buf[..header.payload_length];
        *buf = &buf[header.payload_length..];

        let target = Decodable::decode(&mut list_slice)?;
        let expiration = Decodable::decode(&mut list_slice)?;

        Ok(Self { target, expiration })
    }
}

#[derive(Clone, Debug)]
pub struct Neighbor {
    pub ip: IpAddr,
    pub udp_port: u16,
    pub tcp_port: u16,
    pub id: B512,
}

impl Encodable for Neighbor {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let h = Header { list: true, payload_length: self.payload_length() };
        h.encode(out);
        match self.ip {
            IpAddr::V4(ip) => {
                ip.octets().as_slice().encode(out);
            }
            IpAddr::V6(ip) => {
                ip.octets().as_slice().encode(out);
            }
        }
        self.udp_port.encode(out);
        self.tcp_port.encode(out);
        self.id.encode(out);
    }

    fn length(&self) -> usize {
        let len = self.payload_length();
        alloy_rlp::length_of_length(len) + len
    }
}

impl Neighbor {
    fn payload_length(&self) -> usize {
        let ip_len = match self.ip {
            IpAddr::V4(ip) => ip.octets().as_slice().length(),
            IpAddr::V6(ip) => ip.octets().as_slice().length(),
        };
        ip_len + self.udp_port.length() + self.tcp_port.length() + self.id.length()
    }
}

impl Decodable for Neighbor {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let mut list_slice = &buf[..header.payload_length];
        *buf = &buf[header.payload_length..];

        // Decode IP as RLP string
        let ip_header = Header::decode(&mut list_slice)?;
        if ip_header.list {
            return Err(alloy_rlp::Error::UnexpectedList);
        }
        let ip_bytes = &list_slice[..ip_header.payload_length];
        list_slice = &list_slice[ip_header.payload_length..];

        let ip = if ip_bytes.len() == 4 {
            let mut octets = [0u8; 4];
            octets.copy_from_slice(ip_bytes);
            IpAddr::V4(Ipv4Addr::from(octets))
        } else if ip_bytes.len() == 16 {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(ip_bytes);
            IpAddr::V6(Ipv6Addr::from(octets))
        } else {
            return Err(alloy_rlp::Error::Custom("Invalid IP address length"));
        };
        let udp_port: u16 = Decodable::decode(&mut list_slice)?;
        let tcp_port: u16 = Decodable::decode(&mut list_slice)?;
        let id: B512 = Decodable::decode(&mut list_slice)?;
        Ok(Self { ip, udp_port, tcp_port, id })
    }
}

#[derive(Clone, Debug)]
pub struct Neighbors {
    pub nodes: Vec<Neighbor>,
    pub expiration: u64,
}

impl Encodable for Neighbors {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let payload_len = self.nodes.length() + self.expiration.length();
        let h = Header { list: true, payload_length: payload_len };
        h.encode(out);
        self.nodes.encode(out);
        self.expiration.encode(out);
    }

    fn length(&self) -> usize {
        let payload_len = self.nodes.length() + self.expiration.length();
        alloy_rlp::length_of_length(payload_len) + payload_len
    }
}

impl Decodable for Neighbors {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let mut list_slice = &buf[..header.payload_length];
        *buf = &buf[header.payload_length..];

        let nodes: Vec<Neighbor> = Decodable::decode(&mut list_slice)?;
        let expiration: u64 = Decodable::decode(&mut list_slice)?;
        
        Ok(Self { nodes, expiration })
    }
}

#[derive(Clone, Debug)]
pub struct ENRRequest {
    pub expiration: u64,
}

impl Encodable for ENRRequest {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let payload_len = self.expiration.length();
        let h = Header { list: true, payload_length: payload_len };
        h.encode(out);
        self.expiration.encode(out);
    }

    fn length(&self) -> usize {
        let payload_len = self.expiration.length();
        alloy_rlp::length_of_length(payload_len) + payload_len
    }
}

impl Decodable for ENRRequest {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let mut list_slice = &buf[..header.payload_length];
        *buf = &buf[header.payload_length..];

        let expiration = Decodable::decode(&mut list_slice)?;
        Ok(Self { expiration })
    }
}

#[derive(Clone, Debug)]
pub struct ENRResponse {
    pub reply_token: B256,
    pub enr: Enr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enr {
    pub seq: u64,
    pub signature: [u8; 64],
    pub data: Vec<(String, Vec<u8>)>,
}

impl Enr {
    pub fn new(seq: u64, key: &k256::ecdsa::SigningKey, mut data: Vec<(String, Vec<u8>)>) -> Self {
        data.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

        let mut rlp_content = Vec::new();
        // The content for signing is rlp([seq, k, v, ...])
        let header = alloy_rlp::Header {
            list: true,
            payload_length: Self::content_payload_length(seq, &data),
        };
        header.encode(&mut rlp_content);
        seq.encode(&mut rlp_content);
        for (k, v) in &data {
            k.as_str().encode(&mut rlp_content);
            // v is raw bytes, encode as string (byte slice)
            alloy_rlp::Header { list: false, payload_length: v.len() }.encode(&mut rlp_content);
            rlp_content.put_slice(v);
        }

        let signing_hash = alloy_primitives::keccak256(&rlp_content);
        let sig: k256::ecdsa::Signature = key.sign_prehash(&signing_hash.0).expect("Sign failed");
        
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&sig.to_bytes());

        Self {
            seq,
            signature: sig_bytes,
            data,
        }
    }

    fn content_payload_length(seq: u64, data: &[(String, Vec<u8>)]) -> usize {
        let mut len = seq.length();
        for (k, v) in data {
            len += k.as_str().length();
            // v is encoded as raw bytes string
            len += alloy_rlp::Encodable::length(&v.as_slice());
        }
        len
    }

    pub fn to_base64(&self) -> String {
        let mut buf = Vec::new();
        alloy_rlp::Encodable::encode(self, &mut buf);
        base64::engine::general_purpose::STANDARD.encode(&buf)
    }
}

impl alloy_rlp::Encodable for Enr {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let payload_length = alloy_rlp::Encodable::length(&self.signature.as_slice()) 
            + self.seq.length() 
            + self.data.iter().map(|(k, v)| k.as_str().length() + alloy_rlp::Encodable::length(&v.as_slice())).sum::<usize>();
        
        alloy_rlp::Header {
            list: true,
            payload_length,
        }.encode(out);

        alloy_rlp::Encodable::encode(&self.signature.as_slice(), out);
        self.seq.encode(out);
        for (k, v) in &self.data {
            k.as_str().encode(out);
            // v is raw bytes of the value, we should encode it as a string (byte slice)
            alloy_rlp::Header { list: false, payload_length: v.len() }.encode(out);
            out.put_slice(v);
        }
    }

    fn length(&self) -> usize {
        let payload_length = alloy_rlp::Encodable::length(&self.signature.as_slice()) 
            + self.seq.length() 
            + self.data.iter().map(|(k, v)| k.as_str().length() + alloy_rlp::Encodable::length(&v.as_slice())).sum::<usize>();

        alloy_rlp::length_of_length(payload_length) + payload_length
    }
}

impl Encodable for ENRResponse {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let payload_len = self.reply_token.length() + alloy_rlp::Encodable::length(&self.enr);
        let h = Header { list: true, payload_length: payload_len };
        h.encode(out);
        self.reply_token.encode(out);
        alloy_rlp::Encodable::encode(&self.enr, out);
    }

    fn length(&self) -> usize {
        let payload_len = self.reply_token.length() + alloy_rlp::Encodable::length(&self.enr);
        alloy_rlp::length_of_length(payload_len) + payload_len
    }
}

impl Decodable for ENRResponse {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let mut list_slice = &buf[..header.payload_length];
        *buf = &buf[header.payload_length..];

        let reply_token = Decodable::decode(&mut list_slice)?;
        let enr = Decodable::decode(&mut list_slice)?;
        
        Ok(Self { reply_token, enr })
    }
}

impl alloy_rlp::Decodable for Enr {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = alloy_rlp::Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let mut list_slice = &buf[..header.payload_length];
        *buf = &buf[header.payload_length..];

        let signature: [u8; 64] = alloy_rlp::Decodable::decode(&mut list_slice)?;
        let seq = alloy_rlp::Decodable::decode(&mut list_slice)?;
        
        let mut data = Vec::new();
        while !list_slice.is_empty() {
            let k: String = alloy_rlp::Decodable::decode(&mut list_slice)?;
            let v: Vec<u8> = alloy_rlp::Decodable::decode(&mut list_slice)?;
            data.push((k, v));
        }
        
        Ok(Enr {
            seq,
            signature,
            data,
        })
    }
}

pub fn is_likely_rlp_list(data: &[u8]) -> bool {
    matches!(data.first(), Some(b) if *b >= 0xc0)
}
