use screen_mirror::transport::rtp::{RtpHeader, RtpPacket, H264Packetizer, H264Depacketizer};

#[test]
fn test_rtp_header_roundtrip() {
    let header = RtpHeader {
        version: 2,
        padding: false,
        extension: false,
        marker: true,
        payload_type: 96,
        sequence_number: 1234,
        timestamp: 90000,
        ssrc: 0xDEADBEEF,
    };
    let bytes = header.serialize();
    let parsed = RtpHeader::parse(&bytes).unwrap();
    assert_eq!(parsed.version, 2);
    assert_eq!(parsed.marker, true);
    assert_eq!(parsed.payload_type, 96);
    assert_eq!(parsed.sequence_number, 1234);
    assert_eq!(parsed.timestamp, 90000);
    assert_eq!(parsed.ssrc, 0xDEADBEEF);
}

#[test]
fn test_single_nal_unit_no_fragmentation() {
    let mut packetizer = H264Packetizer::new(96, 0xAABBCCDD, 1400);
    let nal = vec![0x65; 100]; // IDR slice, 100 bytes
    let packets = packetizer.packetize(&nal, 90000);
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].header.payload_type, 96);
    assert_eq!(packets[0].header.timestamp, 90000);
    assert_eq!(packets[0].header.marker, true);
    assert_eq!(packets[0].payload, nal);
}

#[test]
fn test_large_nal_fua_fragmentation() {
    let mut packetizer = H264Packetizer::new(96, 0xAABBCCDD, 1400);
    let nal = vec![0x65; 5000]; // 5000 bytes IDR
    let packets = packetizer.packetize(&nal, 90000);
    assert!(packets.len() >= 4);
    for pkt in &packets[..packets.len() - 1] {
        assert_eq!(pkt.header.marker, false);
    }
    assert_eq!(packets.last().unwrap().header.marker, true);
    for pkt in &packets {
        assert_eq!(pkt.header.timestamp, 90000);
    }
}

#[test]
fn test_fua_roundtrip() {
    let mut packetizer = H264Packetizer::new(96, 0xAABBCCDD, 1400);
    let mut depacketizer = H264Depacketizer::new();

    let original_nal = vec![0x65; 5000];
    let packets = packetizer.packetize(&original_nal, 90000);

    for pkt in &packets {
        depacketizer.push(pkt);
    }
    let reassembled = depacketizer.pop_nal().unwrap();
    assert_eq!(reassembled, original_nal);
}
