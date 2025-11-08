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

impl Default for StompCodec {
    fn default() -> Self {
        Self::new()
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
        // We need to parse headers first to check for an optional Content-length header
        // Find the separator between headers and body: "\n\n"
        let buf = src.as_ref();
        let sep = b"\n\n";
        if let Some(sep_pos) = buf.windows(sep.len()).position(|w| w == sep) {
            // sep_pos is the index of the first '\n' in the separator
            // compute header slice boundaries
            // command ends at first LF (or end)
            let command_end = buf.iter().position(|&b| b == b'\n').unwrap_or(buf.len());

            // header slice is between command_end+1 and sep_pos
            let header_start = if command_end < buf.len() { command_end + 1 } else { command_end };
            let header_slice = if sep_pos > header_start {
                &buf[header_start..sep_pos]
            } else {
                &[][..]
            };

            // Parse headers and look for content-length (case-insensitive)
            let mut headers: Vec<(String, String)> = Vec::new();
            let mut content_length: Option<usize> = None;

            if !header_slice.is_empty() {
                for line in header_slice.split(|&b| b == b'\n') {
                    if line.is_empty() {
                        continue;
                    }
                    if let Some(colon_pos) = line.iter().position(|&b| b == b':') {
                        let k = String::from_utf8(line[..colon_pos].to_vec()).map_err(|e| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("invalid utf8 in header key: {}", e),
                            )
                        })?;
                        let v = String::from_utf8(line[colon_pos + 1..].to_vec()).map_err(|e| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("invalid utf8 in header value: {}", e),
                            )
                        })?;
                        if k.to_lowercase() == "content-length" {
                            // parse integer
                            match v.trim().parse::<usize>() {
                                Ok(n) => content_length = Some(n),
                                Err(_) => {
                                    return Err(io::Error::new(
                                        io::ErrorKind::InvalidData,
                                        format!("invalid content-length value: {}", v),
                                    ));
                                }
                            }
                        }
                        headers.push((k, v));
                    }
                }
            }

            if let Some(clen) = content_length {
                // need: sep_pos + sep.len() + clen + 1 (NUL)
                let total_needed = sep_pos + sep.len() + clen + 1;
                if buf.len() < total_needed {
                    // wait for more data
                    return Ok(None);
                }

                // check NUL terminator at expected position
                let nul_pos = sep_pos + sep.len() + clen;
                if buf[nul_pos] != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "missing NUL terminator after content-length body",
                    ));
                }

                // We have the entire frame up to the NUL; take those bytes
                let frame_bytes = src.split_to(nul_pos);
                // discard the NUL
                src.advance(1);

                // Convert to owned raw buffer for parsing command/body
                let raw = frame_bytes.to_vec();
                if raw.is_empty() {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "empty frame"));
                }

                let command_end = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len());
                let command = String::from_utf8(raw[..command_end].to_vec()).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid utf8 in command: {}", e),
                    )
                })?;

                // headers were already parsed, compute body start offset
                let body_start_rel = sep_pos + sep.len();
                let body = raw[body_start_rel..].to_vec();

                let frame = Frame {
                    command,
                    headers,
                    body,
                };
                return Ok(Some(StompItem::Frame(frame)));
            } else {
                // No content-length: fall back to NUL-terminated frame parsing.
                if let Some(nul_pos) = buf.iter().position(|&b| b == 0) {
                    let frame_bytes = src.split_to(nul_pos);
                    src.advance(1);

                    let raw = frame_bytes.to_vec();
                    if raw.is_empty() {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "empty frame"));
                    }

                    let command_end = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len());
                    let command = String::from_utf8(raw[..command_end].to_vec()).map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("invalid utf8 in command: {}", e),
                        )
                    })?;

                    // find the index of the blank line separating headers and body
                    let sep = b"\n\n";
                    let body_start = raw
                        .windows(sep.len())
                        .position(|w| w == sep)
                        .map(|i| i + sep.len());

                    let mut headers = Vec::new();
                    if let Some(body_off) = body_start {
                        let header_slice = &raw[command.len() + 1..body_off - sep.len()];
                        if !header_slice.is_empty() {
                            for line in header_slice.split(|&b| b == b'\n') {
                                if line.is_empty() {
                                    continue;
                                }
                                if let Some(colon_pos) = line.iter().position(|&b| b == b':') {
                                    let k = String::from_utf8(line[..colon_pos].to_vec()).map_err(|e| {
                                        io::Error::new(
                                            io::ErrorKind::InvalidData,
                                            format!("invalid utf8 in header key: {}", e),
                                        )
                                    })?;
                                    let v = String::from_utf8(line[colon_pos + 1..].to_vec()).map_err(|e| {
                                        io::Error::new(
                                            io::ErrorKind::InvalidData,
                                            format!("invalid utf8 in header value: {}", e),
                                        )
                                    })?;
                                    headers.push((k, v));
                                }
                            }
                        }

                        let body = raw[body_off..].to_vec();

                        let frame = Frame {
                            command,
                            headers,
                            body,
                        };
                        return Ok(Some(StompItem::Frame(frame)));
                    } else {
                        // No separator found; treat entire remaining as command-only frame (no headers/body)
                        let frame = Frame {
                            command,
                            headers: Vec::new(),
                            body: Vec::new(),
                        };
                        return Ok(Some(StompItem::Frame(frame)));
                    }
                }
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
