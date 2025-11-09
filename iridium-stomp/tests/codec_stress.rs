use bytes::BytesMut;
use iridium_stomp::codec::{StompCodec, StompItem};
use iridium_stomp::frame::Frame;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use tokio_util::codec::{Decoder, Encoder};

// Multi-producer stress test: N producers each generate M frames, encode them
// and send random-sized chunks into a single decoder task via an mpsc channel.
// The decoder task appends chunks to a local buffer and decodes frames as they
// become available. This exercises the codec under concurrent producers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn codec_concurrent_stress() {
    let producers = 4usize;
    let per_producer = 80usize;
    let total = producers * per_producer;

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(1024);

    // Spawn producers
    for p in 0..producers {
        let mut tx = tx.clone();
        tokio::spawn(async move {
            let mut rng = StdRng::from_seed([p as u8; 32]);
            let mut codec = StompCodec::new();
            for i in 0..per_producer {
                // sometimes binary, sometimes utf8
                let body = if rng.gen_bool(0.2) {
                    // binary: generate 16 random bytes
                    let mut v = Vec::with_capacity(16);
                    for _ in 0..16 {
                        v.push(rng.gen_range(0..=255) as u8);
                    }
                    v
                } else {
                    format!("producer-{}-msg-{}", p, i).into_bytes()
                };

                let frame = Frame::new("SEND").set_body(body);
                let mut enc = BytesMut::new();
                codec.encode(StompItem::Frame(frame), &mut enc).expect("encode");

                // Send the entire encoded frame as a single chunk. This still
                // allows interleaving between frames from different producers
                // while avoiding pathological mid-frame splits that make the
                // combined stream impossible to decode sensibly.
                if tx.send(enc.to_vec()).await.is_err() {
                    return; // receiver closed
                }
                // occasional tiny yield to increase interleaving
                if rng.gen_bool(0.1) {
                    tokio::task::yield_now().await;
                }
            }
        });
    }

    // drop original tx so channel closes after producers finish
    drop(tx);

    // Decoder task: collect chunks and decode. Keep a record of received
    // chunks so we can dump them when a decoder error occurs for diagnosis.
    let decode_task = tokio::spawn(async move {
        let mut dec = StompCodec::new();
        let mut buf = BytesMut::new();
        let mut decoded = 0usize;
        let mut chunks_received: Vec<Vec<u8>> = Vec::new();
        while let Some(chunk) = rx.recv().await {
            chunks_received.push(chunk.clone());
            buf.extend_from_slice(&chunk);
            loop {
                match dec.decode(&mut buf) {
                    Ok(Some(StompItem::Frame(_))) => decoded += 1,
                    Ok(Some(StompItem::Heartbeat)) => {},
                    Ok(None) => break,
                    Err(e) => {
                        eprintln!("decoder error: {}", e);
                        eprintln!("assembled buf (len={}):\n{:02x?}", buf.len(), &buf[..std::cmp::min(buf.len(), 256)]);
                        eprintln!("received {} chunks; lengths: {:?}", chunks_received.len(), chunks_received.iter().map(|c| c.len()).collect::<Vec<_>>() );
                        // print first few chunks hex
                        for (i, c) in chunks_received.iter().enumerate().take(20) {
                            eprintln!("chunk[{}] (len={}): {:02x?}", i, c.len(), &c[..std::cmp::min(c.len(), 64)]);
                        }
                        panic!("decoder error: {}", e);
                    }
                }
            }
        }
        // Drain any remaining frames after channel closed
        loop {
            match dec.decode(&mut buf) {
                Ok(Some(StompItem::Frame(_))) => decoded += 1,
                Ok(Some(StompItem::Heartbeat)) => {},
                Ok(None) => break,
                Err(e) => {
                    eprintln!("decoder error during drain: {}", e);
                    eprintln!("assembled buf (len={}):\n{:02x?}", buf.len(), &buf[..std::cmp::min(buf.len(), 256)]);
                    panic!("decoder error during drain: {}", e);
                }
            }
        }
        decoded
    });

    // Timeout the entire test to avoid hangs if something goes wrong
    let res = timeout(Duration::from_secs(10), decode_task).await;
    let decoded = match res {
        Ok(join_res) => join_res.expect("decoder task panicked"),
        Err(_) => panic!("stress test timed out"),
    };

    assert_eq!(decoded, total, "decoded count mismatch");
}
