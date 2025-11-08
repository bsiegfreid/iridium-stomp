use bytes::{Buf, BufMut, BytesMut};
use std::io;
use tokio_util::codec::{Decoder, Encoder};

use crate::frame::Frame;

type Headers = Vec<(String, String)>;

fn parse_headers(header_slice: &[u8]) -> Result<(Headers, Option<usize>), io::Error> {
    let mut headers: Headers = Vec::new();
    let mut content_length: Option<usize> = None;

    if header_slice.is_empty() {
        return Ok((headers, content_length));
    }

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
            if k.to_lowercase() == "content-length" {
                let parsed = v.trim().parse::<usize>().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("invalid content-length value: {}", v))
                })?;
                content_length = Some(parsed);
            }
            headers.push((k, v));
        }
    }

    Ok((headers, content_length))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StompItem {
    Frame(Frame),
    Heartbeat,
}

pub struct StompCodec {}

impl StompCodec {
    pub fn new() -> Self { Self {} }
}

impl Default for StompCodec { fn default() -> Self { Self::new() } }

impl Decoder for StompCodec {
    type Item = StompItem;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Heartbeat (single LF)
        if !src.is_empty() && src[0] == b'\n' { src.advance(1); return Ok(Some(StompItem::Heartbeat)); }

        let buf = src.as_ref();
        let sep = b"\n\n";

        // Need header/body separator to parse headers
        let sep_pos = match buf.windows(sep.len()).position(|w| w == sep) { Some(p) => p, None => return Ok(None) };

        let command_end = buf.iter().position(|&b| b == b'\n').unwrap_or(buf.len());
        let header_start = if command_end < buf.len() { command_end + 1 } else { command_end };
        let header_slice = if sep_pos > header_start { &buf[header_start..sep_pos] } else { &[][..] };

        let (headers, content_length) = parse_headers(header_slice)?;

        if let Some(clen) = content_length {
            let needed = sep_pos + sep.len() + clen + 1;
            if buf.len() < needed { return Ok(None); }
            let nul_pos = sep_pos + sep.len() + clen;
            if buf[nul_pos] != 0 { return Err(io::Error::new(io::ErrorKind::InvalidData, "missing NUL after content-length body")); }

            let frame_bytes = src.split_to(nul_pos);
            src.advance(1);

            let raw = frame_bytes.to_vec();
            if raw.is_empty() { return Err(io::Error::new(io::ErrorKind::InvalidData, "empty frame")); }

            let cmd_end = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len());
            let command = String::from_utf8(raw[..cmd_end].to_vec()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid utf8 in command: {}", e)))?;

            let body_start = sep_pos + sep.len();
            let body = raw[body_start..].to_vec();

            let frame = Frame { command, headers, body };
            return Ok(Some(StompItem::Frame(frame)));
        }

        // fallback: NUL-terminated
        if let Some(nul_pos) = buf.iter().position(|&b| b == 0) {
            let frame_bytes = src.split_to(nul_pos);
            src.advance(1);

            let raw = frame_bytes.to_vec();
            if raw.is_empty() { return Err(io::Error::new(io::ErrorKind::InvalidData, "empty frame")); }

            let cmd_end = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len());
            let command = String::from_utf8(raw[..cmd_end].to_vec()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid utf8 in command: {}", e)))?;

            let header_slice = if sep_pos > cmd_end + 1 { &raw[cmd_end + 1..sep_pos] } else { &[][..] };
            let (headers, _cl) = parse_headers(header_slice)?;
            let body = raw[sep_pos + sep.len()..].to_vec();

            let frame = Frame { command, headers, body };
            return Ok(Some(StompItem::Frame(frame)));
        }

        Ok(None)
    }
}

impl Encoder<StompItem> for StompCodec {
    type Error = io::Error;

    fn encode(&mut self, item: StompItem, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            StompItem::Heartbeat => { dst.put_u8(b'\n'); }
            StompItem::Frame(frame) => {
                dst.extend_from_slice(frame.command.as_bytes());
                dst.put_u8(b'\n');

                let mut headers = frame.headers;
                let has_cl = headers.iter().any(|(k, _)| k.to_lowercase() == "content-length");
                if !has_cl {
                    let include_cl = frame.body.contains(&0) || std::str::from_utf8(&frame.body).is_err();
                    if include_cl { headers.push(("content-length".to_string(), frame.body.len().to_string())); }
                }

                for (k, v) in headers {
                    dst.extend_from_slice(k.as_bytes());
                    dst.put_u8(b':');
                    dst.extend_from_slice(v.as_bytes());
                    dst.put_u8(b'\n');
                }

                dst.put_slice(b"\n");
                dst.extend_from_slice(&frame.body);
                dst.put_u8(0);
            }
        }

        Ok(())
    }
}
