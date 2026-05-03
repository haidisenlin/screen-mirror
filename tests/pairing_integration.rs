use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;

use screen_mirror::security::pairing;
use screen_mirror::security::cipher::Cipher;
use screen_mirror::security::replay::ReplayWindow;
use screen_mirror::protocol::session::SecureChannel;
use screen_mirror::protocol::control::{ControlMessage, DisconnectReason};
use screen_mirror::protocol::negotiate::*;

#[test]
fn full_pairing_and_encrypted_exchange() {
    let pin = "482910";

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let receiver_thread = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();

        // Read pA
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).unwrap();
        let pa_len = u32::from_be_bytes(len_buf) as usize;
        let mut msg_a = vec![0u8; pa_len];
        stream.read_exact(&mut msg_a).unwrap();

        // Send pB
        let (msg_b, state) = pairing::receiver_start(pin);
        stream.write_all(&(msg_b.len() as u32).to_be_bytes()).unwrap();
        stream.write_all(&msg_b).unwrap();

        let keys = pairing::receiver_finish(state, &msg_a).unwrap();
        let mut channel = SecureChannel::new(stream, &keys.control_key);

        // Receive offer
        let offer_bytes = channel.recv().unwrap().unwrap();
        let offer_msg = NegotiateMessage::from_bytes(&offer_bytes).unwrap();
        assert!(matches!(offer_msg, NegotiateMessage::Offer(_)));

        // Send answer
        let answer = Answer {
            video: AnswerVideo {
                codec: "h264".to_string(),
                max_width: 1920,
                max_height: 1080,
                max_fps: 60,
            },
            audio: AnswerAudio {
                codec: "opus".to_string(),
                sample_rate: 48000,
                channels: 2,
            },
            transport: AnswerTransport { udp_port: 6000 },
        };
        channel.send(&NegotiateMessage::Answer(answer).to_bytes()).unwrap();

        // Receive ping
        let msg_bytes = channel.recv().unwrap().unwrap();
        let msg = ControlMessage::from_bytes(&msg_bytes).unwrap();
        assert_eq!(msg, ControlMessage::Ping);

        // Send pong
        channel.send(&ControlMessage::Pong.to_bytes()).unwrap();

        keys.media_key
    });

    // Sender side
    let mut stream = TcpStream::connect(addr).unwrap();

    let (msg_a, state) = pairing::sender_start(pin);
    stream.write_all(&(msg_a.len() as u32).to_be_bytes()).unwrap();
    stream.write_all(&msg_a).unwrap();

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).unwrap();
    let pb_len = u32::from_be_bytes(len_buf) as usize;
    let mut msg_b = vec![0u8; pb_len];
    stream.read_exact(&mut msg_b).unwrap();

    let keys = pairing::sender_finish(state, &msg_b).unwrap();
    let mut channel = SecureChannel::new(stream, &keys.control_key);

    // Send offer
    let offer = Offer {
        video: OfferVideo {
            codec: "h264".to_string(),
            width: 2560,
            height: 1440,
            fps: 60,
            bitrate: 20_000_000,
        },
        audio: OfferAudio {
            codec: "opus".to_string(),
            sample_rate: 48000,
            channels: 2,
            bitrate: 128_000,
        },
        transport: OfferTransport {
            udp_port: 5004,
            fec_group_size: 6,
        },
    };
    channel.send(&NegotiateMessage::Offer(offer).to_bytes()).unwrap();

    // Receive answer
    let answer_bytes = channel.recv().unwrap().unwrap();
    let answer_msg = NegotiateMessage::from_bytes(&answer_bytes).unwrap();
    assert!(matches!(answer_msg, NegotiateMessage::Answer(_)));

    // Send ping
    channel.send(&ControlMessage::Ping.to_bytes()).unwrap();

    // Receive pong
    let pong_bytes = channel.recv().unwrap().unwrap();
    let pong = ControlMessage::from_bytes(&pong_bytes).unwrap();
    assert_eq!(pong, ControlMessage::Pong);

    // Verify media key agreement
    let receiver_media_key = receiver_thread.join().unwrap();
    assert_eq!(keys.media_key, receiver_media_key);

    // Test media cipher roundtrip
    let mut sender_media = Cipher::new(&keys.media_key, [0, 0, 0, 1]);
    let receiver_media = Cipher::new(&receiver_media_key, [0, 0, 0, 1]);
    let mut replay = ReplayWindow::new();

    let fake_rtp = b"\x80\x60\x00\x01\x00\x00\x00\x00\x12\x34\x56\x78payload";
    let encrypted = sender_media.seal(fake_rtp);
    let (counter, decrypted) = receiver_media.open(&encrypted).unwrap();
    assert!(replay.check_and_mark(counter));
    assert_eq!(decrypted, fake_rtp);
}
