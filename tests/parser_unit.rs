use iridium_stomp::parser::parse_frame_slice;

#[test]
fn parse_frame_slice_invalid_content_length() {
    let raw = b"SEND\ncontent-length:xyz\n\nhello\0".to_vec();
    match parse_frame_slice(&raw) {
        Err(_) => {}
        Ok(Some(_)) => panic!("expected error for invalid content-length"),
        Ok(None) => panic!("expected error, got None (need more bytes)"),
    }
}
