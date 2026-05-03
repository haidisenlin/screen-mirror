#[derive(Debug, Clone, PartialEq)]
pub struct RtpHeader {
    pub version: u8,
    pub padding: bool,
    pub extension: bool,
    pub marker: bool,
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
}

impl RtpHeader {
    pub const SIZE: usize = 12;

    pub fn serialize(&self) -> [u8; 12] {
        let mut buf = [0u8; 12];
        buf[0] = (self.version << 6)
            | ((self.padding as u8) << 5)
            | ((self.extension as u8) << 4);
        buf[1] = ((self.marker as u8) << 7) | (self.payload_type & 0x7F);
        buf[2..4].copy_from_slice(&self.sequence_number.to_be_bytes());
        buf[4..8].copy_from_slice(&self.timestamp.to_be_bytes());
        buf[8..12].copy_from_slice(&self.ssrc.to_be_bytes());
        buf
    }

    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 12 {
            return None;
        }
        Some(Self {
            version: (buf[0] >> 6) & 0x03,
            padding: (buf[0] >> 5) & 0x01 == 1,
            extension: (buf[0] >> 4) & 0x01 == 1,
            marker: (buf[1] >> 7) & 0x01 == 1,
            payload_type: buf[1] & 0x7F,
            sequence_number: u16::from_be_bytes([buf[2], buf[3]]),
            timestamp: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
            ssrc: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
        })
    }
}

#[derive(Debug, Clone)]
pub struct RtpPacket {
    pub header: RtpHeader,
    pub payload: Vec<u8>,
}

impl RtpPacket {
    pub fn serialize(&self) -> Vec<u8> {
        let header_bytes = self.header.serialize();
        let mut buf = Vec::with_capacity(12 + self.payload.len());
        buf.extend_from_slice(&header_bytes);
        buf.extend_from_slice(&self.payload);
        buf
    }

    pub fn parse(buf: &[u8]) -> Option<Self> {
        let header = RtpHeader::parse(buf)?;
        let payload = buf[12..].to_vec();
        Some(Self { header, payload })
    }
}

pub struct H264Packetizer {
    payload_type: u8,
    ssrc: u32,
    max_payload: usize,
    sequence_number: u16,
}

impl H264Packetizer {
    pub fn new(payload_type: u8, ssrc: u32, mtu: usize) -> Self {
        Self {
            payload_type,
            ssrc,
            max_payload: mtu - RtpHeader::SIZE,
            sequence_number: 0,
        }
    }

    pub fn packetize(&mut self, nal: &[u8], timestamp: u32) -> Vec<RtpPacket> {
        if nal.len() <= self.max_payload {
            self.single_nal(nal, timestamp)
        } else {
            self.fragment_fua(nal, timestamp)
        }
    }

    fn next_seq(&mut self) -> u16 {
        let seq = self.sequence_number;
        self.sequence_number = self.sequence_number.wrapping_add(1);
        seq
    }

    fn single_nal(&mut self, nal: &[u8], timestamp: u32) -> Vec<RtpPacket> {
        vec![RtpPacket {
            header: RtpHeader {
                version: 2,
                padding: false,
                extension: false,
                marker: true,
                payload_type: self.payload_type,
                sequence_number: self.next_seq(),
                timestamp,
                ssrc: self.ssrc,
            },
            payload: nal.to_vec(),
        }]
    }

    fn fragment_fua(&mut self, nal: &[u8], timestamp: u32) -> Vec<RtpPacket> {
        let nal_header = nal[0];
        let nal_type = nal_header & 0x1F;
        let nri = nal_header & 0x60;
        let data = &nal[1..];

        let fua_max = self.max_payload - 2; // 2 bytes for FU indicator + FU header
        let mut packets = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            let end = (offset + fua_max).min(data.len());
            let is_start = offset == 0;
            let is_end = end == data.len();

            let fu_indicator = 28 | nri; // FU-A type = 28
            let fu_header = ((is_start as u8) << 7) | ((is_end as u8) << 6) | nal_type;

            let mut payload = Vec::with_capacity(2 + (end - offset));
            payload.push(fu_indicator);
            payload.push(fu_header);
            payload.extend_from_slice(&data[offset..end]);

            packets.push(RtpPacket {
                header: RtpHeader {
                    version: 2,
                    padding: false,
                    extension: false,
                    marker: is_end,
                    payload_type: self.payload_type,
                    sequence_number: self.next_seq(),
                    timestamp,
                    ssrc: self.ssrc,
                },
                payload,
            });
            offset = end;
        }
        packets
    }
}

#[derive(Default)]
pub struct H264Depacketizer {
    buffer: Vec<u8>,
    in_fragment: bool,
}

impl H264Depacketizer {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            in_fragment: false,
        }
    }

    pub fn push(&mut self, packet: &RtpPacket) {
        if packet.payload.is_empty() {
            return;
        }
        let first_byte = packet.payload[0];
        let nal_type = first_byte & 0x1F;

        if nal_type == 28 {
            // FU-A
            if packet.payload.len() < 2 {
                return;
            }
            let fu_header = packet.payload[1];
            let is_start = (fu_header >> 7) & 1 == 1;
            let original_type = fu_header & 0x1F;
            let nri = first_byte & 0x60;

            if is_start {
                self.buffer.clear();
                self.buffer.push(nri | original_type);
                self.in_fragment = true;
            }
            if self.in_fragment {
                self.buffer.extend_from_slice(&packet.payload[2..]);
            }
        } else {
            // Single NAL unit
            self.buffer = packet.payload.clone();
            self.in_fragment = false;
        }
    }

    pub fn pop_nal(&mut self) -> Option<Vec<u8>> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.buffer))
        }
    }
}
