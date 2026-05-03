use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::rtp::{RtpHeader, RtpPacket};

const FEC_PT: u8 = 127;

#[derive(Debug, Clone, PartialEq)]
pub struct FecHeader {
    pub base_seq: u16,
    pub group_len: u8,
    pub media_pt: u8,
}

impl FecHeader {
    pub fn serialize(&self) -> [u8; 4] {
        let mut buf = [0u8; 4];
        buf[0..2].copy_from_slice(&self.base_seq.to_be_bytes());
        buf[2] = self.group_len;
        buf[3] = self.media_pt;
        buf
    }

    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 4 {
            return None;
        }
        Some(Self {
            base_seq: u16::from_be_bytes([buf[0], buf[1]]),
            group_len: buf[2],
            media_pt: buf[3],
        })
    }
}

fn rand_ssrc() -> u32 {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u32;
    t ^ 0xDEAD_BEEF
}

fn xor_payloads(payloads: &[Vec<u8>]) -> Vec<u8> {
    let max_len = payloads.iter().map(|p| p.len()).max().unwrap_or(0);
    let mut result = vec![0u8; max_len];
    for payload in payloads {
        for (i, &b) in payload.iter().enumerate() {
            result[i] ^= b;
        }
    }
    result
}

pub struct FecEncoder {
    group_size: usize,
    media_pt: u8,
    ssrc: u32,
    seq: u16,
    buffer: Vec<RtpPacket>,
}

impl FecEncoder {
    pub fn new(group_size: usize, media_pt: u8) -> Self {
        Self {
            group_size,
            media_pt,
            ssrc: rand_ssrc(),
            seq: 0,
            buffer: Vec::with_capacity(group_size),
        }
    }

    pub fn push(&mut self, pkt: &RtpPacket) -> Option<RtpPacket> {
        self.buffer.push(pkt.clone());
        if self.buffer.len() >= self.group_size {
            Some(self.generate_fec())
        } else {
            None
        }
    }

    pub fn flush(&mut self) -> Option<RtpPacket> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(self.generate_fec())
        }
    }

    fn generate_fec(&mut self) -> RtpPacket {
        let base_seq = self.buffer[0].header.sequence_number;
        let group_len = self.buffer.len() as u8;

        let payloads: Vec<Vec<u8>> = self.buffer.iter().map(|p| p.payload.clone()).collect();
        let xor_data = xor_payloads(&payloads);

        let fec_header = FecHeader {
            base_seq,
            group_len,
            media_pt: self.media_pt,
        };

        let mut payload = Vec::with_capacity(4 + xor_data.len());
        payload.extend_from_slice(&fec_header.serialize());
        payload.extend_from_slice(&xor_data);

        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);

        self.buffer.clear();

        RtpPacket {
            header: RtpHeader {
                version: 2,
                padding: false,
                extension: false,
                marker: false,
                payload_type: FEC_PT,
                sequence_number: seq,
                timestamp: 0,
                ssrc: self.ssrc,
            },
            payload,
        }
    }
}

struct FecGroup {
    base_seq: u16,
    group_len: u8,
    xor_payload: Vec<u8>,
    received: HashMap<u16, Vec<u8>>,
}

pub struct FecDecoder {
    groups: Vec<FecGroup>,
}

impl Default for FecDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FecDecoder {
    pub fn new() -> Self {
        Self { groups: Vec::new() }
    }

    pub fn push_media(&mut self, pkt: &RtpPacket) {
        let seq = pkt.header.sequence_number;
        for group in &mut self.groups {
            let end_seq = group.base_seq.wrapping_add(group.group_len as u16);
            if seq_in_range(seq, group.base_seq, end_seq) {
                group.received.insert(seq, pkt.payload.clone());
            }
        }
    }

    pub fn push_fec(&mut self, pkt: &RtpPacket) {
        let Some(fec_header) = FecHeader::parse(&pkt.payload) else {
            return;
        };
        let xor_payload = pkt.payload[4..].to_vec();
        self.groups.push(FecGroup {
            base_seq: fec_header.base_seq,
            group_len: fec_header.group_len,
            xor_payload,
            received: HashMap::new(),
        });
    }

    pub fn recover(&mut self, missing_seq: u16) -> Option<Vec<u8>> {
        for group in &self.groups {
            let end_seq = group.base_seq.wrapping_add(group.group_len as u16);
            if !seq_in_range(missing_seq, group.base_seq, end_seq) {
                continue;
            }

            // Check we have exactly (group_len - 1) packets
            let expected = group.group_len as usize;
            if group.received.len() != expected - 1 {
                continue;
            }

            // Verify the missing one is actually missing
            if group.received.contains_key(&missing_seq) {
                continue;
            }

            // XOR all received payloads with the FEC xor_payload to recover
            let mut payloads: Vec<Vec<u8>> = group.received.values().cloned().collect();
            payloads.push(group.xor_payload.clone());
            return Some(xor_payloads(&payloads));
        }
        None
    }

    pub fn remove_old_groups(&mut self, before_seq: u16) {
        self.groups.retain(|g| {
            // Keep groups whose base_seq is not "before" the cutoff
            !seq_before(g.base_seq, before_seq)
        });
    }
}

/// Check if `seq` is in range [base, end) accounting for wrapping.
fn seq_in_range(seq: u16, base: u16, end: u16) -> bool {
    let offset = seq.wrapping_sub(base);
    let len = end.wrapping_sub(base);
    offset < len
}

/// Check if `a` is strictly before `b` in sequence space.
fn seq_before(a: u16, b: u16) -> bool {
    let diff = a.wrapping_sub(b);
    diff > 0x7FFF
}
