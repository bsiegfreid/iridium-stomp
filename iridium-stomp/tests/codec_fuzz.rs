use bytes::BytesMut;
use iridium_stomp::codec::{StompCodec, StompItem};
use iridium_stomp::frame::Frame;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use tokio_util::codec::{Decoder, Encoder};

/// Encode several frames and feed them to the decoder split into random
/// chunk sizes. The RNG is seeded so the test is deterministic.
#[test]
fn randomized_splits_multiple_frames() {
    let mut codec = StompCodec::new();

    // Build three frames with varying bodies
    let frames = vec![
        Frame::new("SEND").set_body(b"alpha".to_vec()),
        Frame::new("SEND").set_body(vec![0u8, 1, 2, 3, 4]), // binary -> content-length
        Frame::new("SEND").set_body(b"omega".to_vec()),
    ];

    // Encode them back-to-back into a single buffer
    let mut encoded = BytesMut::new();
    for f in frames.iter().cloned() {
        codec.encode(StompItem::Frame(f), &mut encoded).expect("encode");
    }

    // Deterministic RNG
    let mut rng = StdRng::from_seed([0x42; 32]);

    // Split into random chunks and feed to decoder
    let mut chunks: Vec<BytesMut> = Vec::new();
    let mut off = 0usize;
    while off < encoded.len() {
        let sz = (rng.gen_range(1..8)).min(encoded.len() - off);
        let mut c = BytesMut::new();
        c.extend_from_slice(&encoded[off..off + sz]);
        chunks.push(c);
        off += sz;
    }

    let mut dec = StompCodec::new();
    let mut feed = BytesMut::new();
    let mut decoded_count = 0usize;
    for c in chunks {
        feed.extend_from_slice(&c);
        loop {
            match dec.decode(&mut feed) {
                Ok(Some(StompItem::Frame(f))) => {
                    // basic sanity: bodies should match one of the original
                    let b = f.body.clone();
                    assert!(b == b"alpha".to_vec() || b == b"omega".to_vec() || b == vec![0u8,1,2,3,4]);
                    decoded_count += 1;
                }
                Ok(Some(StompItem::Heartbeat)) => { /* ignore */ }
                Ok(None) => break,
                Err(e) => panic!("decoder error: {}", e),
            }
        }
    }

    assert_eq!(decoded_count, 3, "expected to decode three frames");
}

/// Feed a long stream containing many small frames, splitting randomly,
/// to ensure the decoder can sustain streaming workloads.
#[test]
fn streaming_many_small_frames() {
    let mut codec = StompCodec::new();
    let mut encoded = BytesMut::new();
    for i in 0..200 {
        let body = format!("msg-{}", i).into_bytes();
        let f = Frame::new("SEND").set_body(body);
        codec.encode(StompItem::Frame(f), &mut encoded).expect("encode");
    }

    let mut rng = StdRng::from_seed([0x99; 32]);
    // create random chunk sizes
    let mut off = 0usize;
    let mut chunks: Vec<BytesMut> = Vec::new();
    while off < encoded.len() {
        let sz = rng.gen_range(1..64).min(encoded.len() - off);
        let mut c = BytesMut::new();
        c.extend_from_slice(&encoded[off..off + sz]);
        chunks.push(c);
        off += sz;
    }

    let mut dec = StompCodec::new();
    let mut feed = BytesMut::new();
    let mut decoded = 0usize;
    for c in chunks {
        feed.extend_from_slice(&c);
        loop {
            match dec.decode(&mut feed) {
                Ok(Some(StompItem::Frame(_f))) => decoded += 1,
                Ok(Some(StompItem::Heartbeat)) => {},
                Ok(None) => break,
                Err(e) => panic!("decoder error: {}", e),
            }
        }
    }

    assert_eq!(decoded, 200, "expected to decode 200 frames");
}
