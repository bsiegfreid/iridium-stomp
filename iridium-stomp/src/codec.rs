use bytes::{Buf, BufMut, BytesMut};
use std::io;
use tokio_util::codec::{Decoder, Encoder};

use crate::frame::Frame;

/// Items produced/consumed by the codec: either a Frame or a Heartbeat marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StompItem {
    Frame(Frame),
    Heartbeat,
}

pub struct StompCodec {
    // future configuration fields (e.g., max frame size) can go here
}

impl StompCodec {
    pub fn new() -> Self {
        Self {}
    }
}

impl Decoder for StompCodec {
    type Item = StompItem;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Heartbeat is a single LF (\n) byte by convention
        if !src.is_empty() && src[0] == b'\n' {
            src.advance(1);
            return Ok(Some(StompItem::Heartbeat));
        }

        // STOMP frames are terminated by a NUL (0) byte
        if let Some(pos) = src.iter().position(|&b| b == 0) {
            // take the frame bytes up to the NUL
            let frame_bytes = src.split_to(pos);
            // discard the NUL
            src.advance(1);

            // parse frame_bytes: command\nheaders...\n\nbody
            // convert to owned buffer for simpler parsing
            let raw = frame_bytes.to_vec();

            if raw.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "empty frame"));
            }

            // command is the first line (up to the first LF), or the whole raw if no LF
            let command_end = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len());
            let command = String::from_utf8(raw[..command_end].to_vec()).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("invalid utf8 in command: {}", e))
            })?;

            // find the index of the blank line separating headers and body
            let mut headers = Vec::new();
            // find "\n\n" separator
            let sep = b"\n\n";
            let body_start = raw
                .windows(sep.len())
                .position(|w| w == sep)
                .map(|i| i + sep.len());

            if let Some(body_off) = body_start {
                let header_slice = &raw[command.len() + 1..body_off - sep.len()];
                if !header_slice.is_empty() {
                    for line in header_slice.split(|&b| b == b'\n') {
                        if line.is_empty() {
                            continue;
                        }
                        if let Some(colon_pos) = line.iter().position(|&b| b == b':') {
                            let k = String::from_utf8(line[..colon_pos].to_vec()).map_err(|e| {
                                io::Error::new(io::ErrorKind::InvalidData, format!("invalid utf8 in header key: {}", e))
                            })?;
                            let v = String::from_utf8(line[colon_pos + 1..].to_vec()).map_err(|e| {
                                io::Error::new(io::ErrorKind::InvalidData, format!("invalid utf8 in header value: {}", e))
                            })?;
                            headers.push((k, v));
                        }
                    }
                }

                let body = raw[body_off..].to_vec();

                let frame = Frame { command, headers, body };
                return Ok(Some(StompItem::Frame(frame)));
            } else {
                // No separator found; treat entire remaining as command-only frame (no headers/body)
                let frame = Frame { command, headers: Vec::new(), body: Vec::new() };
                return Ok(Some(StompItem::Frame(frame)));
            }
        }

        // not enough data yet
        Ok(None)
    }
}

impl Encoder<StompItem> for StompCodec {
    type Error = io::Error;

    fn encode(&mut self, item: StompItem, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            StompItem::Heartbeat => {
                dst.put_u8(b'\n');
            }
            StompItem::Frame(frame) => {
                dst.extend_from_slice(frame.command.as_bytes());
                dst.put_u8(b'\n');
                for (k, v) in frame.headers {
                    dst.extend_from_slice(k.as_bytes());
                    dst.put_u8(b':');
                    dst.extend_from_slice(v.as_bytes());
                    dst.put_u8(b'\n');
                }
                dst.put_slice(b"\n");
                dst.extend_from_slice(&frame.body);
                dst.put_u8(0); // NUL terminator
            }
        }
        Ok(())
    }
}
