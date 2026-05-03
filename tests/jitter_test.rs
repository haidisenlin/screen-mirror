use screen_mirror::transport::jitter::AudioJitterBuffer;

#[test]
fn test_jitter_buffer_basic_push_pull() {
    let mut buf = AudioJitterBuffer::new(4);
    let frame1 = vec![1.0f32; 480];
    let frame2 = vec![2.0f32; 480];

    buf.push_frame(&frame1);
    buf.push_frame(&frame2);

    let mut out = vec![0.0f32; 480];
    assert!(buf.pull_frame(&mut out));
    assert_eq!(out[0], 1.0);

    assert!(buf.pull_frame(&mut out));
    assert_eq!(out[0], 2.0);

    assert!(!buf.pull_frame(&mut out)); // empty
}

#[test]
fn test_jitter_buffer_overflow_drops_oldest() {
    let mut buf = AudioJitterBuffer::new(2);
    buf.push_frame(&vec![1.0f32; 480]);
    buf.push_frame(&vec![2.0f32; 480]);
    buf.push_frame(&vec![3.0f32; 480]); // overflow, drops frame 1

    let mut out = vec![0.0f32; 480];
    assert!(buf.pull_frame(&mut out));
    assert_eq!(out[0], 2.0);

    assert!(buf.pull_frame(&mut out));
    assert_eq!(out[0], 3.0);
}

#[test]
fn test_jitter_buffer_level() {
    let mut buf = AudioJitterBuffer::new(4);
    assert_eq!(buf.level(), 0);
    buf.push_frame(&vec![0.0f32; 480]);
    assert_eq!(buf.level(), 1);
    buf.push_frame(&vec![0.0f32; 480]);
    assert_eq!(buf.level(), 2);
    let mut out = vec![0.0f32; 480];
    buf.pull_frame(&mut out);
    assert_eq!(buf.level(), 1);
}
