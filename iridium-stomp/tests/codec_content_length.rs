use bytes::BytesMut;
use iridium_stomp::StompCodec;
use tokio_util::codec::Decoder;

#[test]
fn decode_with_content_length_and_nul_in_body() {
    let mut codec = StompCodec::new();

    // command = MESSAGE\n, header content-length:3\n, blank line, body = b"a\0b" (3 bytes), then NUL terminator
    // Build a proper raw buffer: header + body (with embedded NUL) + NUL terminator
    let raw = b"MESSAGE\ncontent-length:3\n\n".iter().chain(b"a\0b").cloned().chain(std::iter::once(0u8)).collect::<Vec<u8>>();
    let mut buf = BytesMut::from(raw.as_slice());

    let item = codec.decode(&mut buf).expect("decode error").expect("no item");
    match item {
        iridium_stomp::StompItem::Frame(f) => {
            assert_eq!(f.command, "MESSAGE");
            // find content-length header
            let mut has_cl = false;
            for (k, v) in f.headers.iter() {
                if k.to_lowercase() == "content-length" {
                    has_cl = true;
                    assert_eq!(v, "3");
                }
            }
            assert!(has_cl, "content-length header missing");
            assert_eq!(f.body, b"a\0b");
        }
        _ => panic!("expected frame"),
    }
}

#[test]
fn invalid_content_length_is_error() {
    let mut codec = StompCodec::new();
    let raw = b"SEND\ncontent-length:xyz\n\nhello\0".to_vec();
    let mut buf = BytesMut::from(raw.as_slice());
    let res = codec.decode(&mut buf);
    assert!(res.is_err(), "invalid content-length should produce error");
}
