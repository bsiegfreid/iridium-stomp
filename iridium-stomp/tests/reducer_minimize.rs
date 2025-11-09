use bytes::BytesMut;
use iridium_stomp::codec::{StompCodec, StompItem};
use tokio_util::codec::Decoder;

// Greedy reducer that attempts to remove chunks from the captured failing
// sequence while preserving the failing behavior (decoded < expected).
//
// This test prints a minimized chunk sequence (as hex arrays) which can be
// used to craft a tiny deterministic regression test.
#[test]
fn minimize_failing_chunk_sequence() {
    // Original captured chunks from failing run (same as regression_replay)
    let original: Vec<Vec<u8>> = vec![
        vec![0x53, 0x45],
        vec![0x4e, 0x44, 0x0a, 0x0a, 0x70, 0x72, 0x6f, 0x64, 0x75, 0x63, 0x65, 0x72, 0x2d, 0x30, 0x2d],
        vec![0x6d, 0x73, 0x67, 0x2d, 0x30, 0x00],
        vec![0x53, 0x45, 0x4e, 0x44, 0x0a, 0x0a, 0x70, 0x72, 0x6f, 0x64, 0x75, 0x63, 0x65, 0x72, 0x2d, 0x30],
        vec![0x53, 0x45, 0x4e, 0x44, 0x0a, 0x0a, 0x70, 0x72, 0x6f, 0x64, 0x75, 0x63, 0x65],
        vec![0x2d, 0x6d],
        vec![0x72, 0x2d, 0x32, 0x2d, 0x6d, 0x73, 0x67, 0x2d, 0x30, 0x00],
        vec![0x73, 0x67, 0x2d, 0x31, 0x00],
        vec![0x53, 0x45, 0x4e],
        vec![0x53, 0x45, 0x4e, 0x44, 0x0a, 0x0a, 0x70, 0x72, 0x6f, 0x64, 0x75, 0x63, 0x65, 0x72, 0x2d, 0x30],
    ];

    let mut chunks: Vec<Vec<u8>> = original.clone();

    // helper to check if a sequence reproduces the problem (decoded < expected)
    fn reproduces(seq: &Vec<Vec<u8>>) -> bool {
        let mut combined = Vec::new();
        for s in seq { combined.extend_from_slice(&s[..]); }
        let expected = combined.iter().filter(|&&b| b == 0).count();

        let mut dec = StompCodec::new();
        let mut buf = BytesMut::new();
        let mut decoded = 0usize;

        for s in seq {
            buf.extend_from_slice(&s[..]);
            loop {
                match dec.decode(&mut buf) {
                    Ok(Some(StompItem::Frame(_))) => decoded += 1,
                    Ok(Some(StompItem::Heartbeat)) => {},
                    Ok(None) => break,
                    Err(_) => return false, // parse error alone is not the original symptom
                }
            }
        }

        loop {
            match dec.decode(&mut buf) {
                Ok(Some(StompItem::Frame(_))) => decoded += 1,
                Ok(Some(StompItem::Heartbeat)) => {},
                Ok(None) => break,
                Err(_) => return false,
            }
        }

        // only treat it as reproduction when at least one frame was decoded
        // but the count is still less than the number of NUL terminators.
        decoded > 0 && decoded < expected
    }

    // ensure original reproduces
    assert!(reproduces(&chunks), "original sequence must reproduce the failure");

    // Find the smallest contiguous window (subsequence of consecutive
    // chunks) that still reproduces the issue. This preserves local context
    // (no removed interior chunks) and yields a compact, realistic repro.
    let n = chunks.len();
    let mut found: Option<Vec<Vec<u8>>> = None;
    'outer: for len in 1..=n {
        for start in 0..=n - len {
            let trial: Vec<Vec<u8>> = chunks[start..start + len].to_vec();
            if reproduces(&trial) {
                found = Some(trial);
                break 'outer;
            }
        }
    }

    let final_chunks = found.expect("no contiguous failing window found");

    eprintln!("minimized contiguous chunks (count={}):", final_chunks.len());
    for c in &final_chunks {
        eprint!("&[");
        for (i, b) in c.iter().enumerate() {
            if i != 0 { eprint!(", "); }
            eprint!("0x{:02x}", b);
        }
        eprintln!("],");
    }

    // sanity: minimized must still reproduce
    assert!(reproduces(&final_chunks), "minimized contiguous sequence did not reproduce");
}
