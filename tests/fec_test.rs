use screen_mirror::transport::fec::{FecDecoder, FecEncoder};
use screen_mirror::transport::rtp::{RtpHeader, RtpPacket};

fn make_rtp(seq: u16, pt: u8, payload: &[u8]) -> RtpPacket {
    RtpPacket {
        header: RtpHeader {
            version: 2,
            padding: false,
            extension: false,
            marker: seq % 6 == 5,
            payload_type: pt,
            sequence_number: seq,
            timestamp: seq as u32 * 3000,
            ssrc: 0xAAAAAAAA,
        },
        payload: payload.to_vec(),
    }
}

#[test]
fn test_fec_encoder_produces_parity_after_group_size() {
    let mut enc = FecEncoder::new(6, 96);
    for i in 0..5u16 {
        let pkt = make_rtp(i, 96, &[i as u8; 100]);
        assert!(enc.push(&pkt).is_none());
    }
    let pkt = make_rtp(5, 96, &[5u8; 100]);
    let fec_pkt = enc.push(&pkt).expect("should produce FEC packet after 6th");
    assert_eq!(fec_pkt.header.payload_type, 127);
}

#[test]
fn test_fec_recovers_single_lost_packet() {
    let mut enc = FecEncoder::new(4, 96);
    let packets: Vec<RtpPacket> = (0..4u16)
        .map(|i| make_rtp(i, 96, &[(i as u8 + 1) * 17; 50]))
        .collect();

    let mut fec_pkt = None;
    for pkt in &packets {
        fec_pkt = enc.push(pkt);
    }
    let fec_pkt = fec_pkt.unwrap();

    // Simulate: packet seq=2 is lost
    let mut dec = FecDecoder::new();
    dec.push_fec(&fec_pkt);
    dec.push_media(&packets[0]);
    dec.push_media(&packets[1]);
    // packets[2] is lost
    dec.push_media(&packets[3]);

    let recovered = dec.recover(2).expect("should recover seq=2");
    assert_eq!(recovered, packets[2].payload);
}

#[test]
fn test_fec_cannot_recover_two_lost_packets() {
    let mut enc = FecEncoder::new(4, 96);
    let packets: Vec<RtpPacket> = (0..4u16).map(|i| make_rtp(i, 96, &[i as u8; 50])).collect();

    let mut fec_pkt = None;
    for pkt in &packets {
        fec_pkt = enc.push(pkt);
    }
    let fec_pkt = fec_pkt.unwrap();

    let mut dec = FecDecoder::new();
    dec.push_fec(&fec_pkt);
    dec.push_media(&packets[0]);
    // packets[1] and packets[2] lost
    dec.push_media(&packets[3]);

    assert!(dec.recover(1).is_none());
    assert!(dec.recover(2).is_none());
}
