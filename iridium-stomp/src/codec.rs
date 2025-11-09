use bytes::{BufMut, BytesMut};
use std::io;
use tokio_util::codec::{Decoder, Encoder};

use crate::frame::Frame;

/// Alias for the collection used to represent STOMP headers.
///
/// Each header is a (key, value) pair using owned `String`s.
type Headers = Vec<(String, String)>;

/// Parse the raw header block from a STOMP frame and extract an optional
/// `content-length` header.
///
/// Parameters
/// - `header_slice`: the bytes between the command's terminating LF and the
///   blank-line separator that precedes the body. This slice may be empty.
///
/// Returns
/// - `Ok((headers, Some(content_length)))` when a `content-length` header was
///   present and parsed successfully.
/// - `Ok((headers, None))` when no `content-length` header was present.
/// - `Err(io::Error)` with `InvalidData` when header keys/values are not valid
///   UTF-8 or when `content-length` is not a valid non-negative integer.
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
                let parsed = v.trim().parse::<usize>().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid content-length value: {}", v),
                    )
                })?;
                content_length = Some(parsed);
            }
            headers.push((k, v));
        }
    }

    Ok((headers, content_length))
}

/// Items produced or consumed by the codec.
///
/// A `StompItem` is either a decoded `Frame` or a `Heartbeat` marker
/// representing a single LF received on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StompItem {
    /// A decoded STOMP frame (command + headers + body)
    Frame(Frame),
    /// A single heartbeat pulse (LF)
    Heartbeat,
}

/// `StompCodec` implements `tokio_util::codec::{Decoder, Encoder}` for the
/// STOMP wire protocol.
///
/// Responsibilities:
/// - Decode incoming bytes into `StompItem::Frame` or `StompItem::Heartbeat`.
/// - Support both NUL-terminated frames and frames using the `content-length`
///   header (STOMP 1.2) for binary bodies containing NUL bytes.
/// - Encode `StompItem` back into bytes for the wire format and emit
///   `content-length` when necessary.
pub struct StompCodec {
    /// Internal buffer which accumulates bytes across incremental `decode`
    /// calls. Using an internal buffer avoids relying on indexes computed
    /// from the `src` argument (which the caller may mutate between calls)
    /// and simplifies parsing across arbitrary chunk boundaries.
    buf: BytesMut,
}

impl StompCodec {
    pub fn new() -> Self {
        Self { buf: BytesMut::new() }
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
    /// Decode bytes from `src` into a `StompItem`.
    ///
    /// Parameters
    /// - `src`: a mutable reference to the read buffer containing bytes from the
    ///   transport. The decoder may consume bytes from this buffer (using
    ///   methods like `advance` or `split_to`) when it successfully decodes a
    ///   frame. If there are not enough bytes to form a complete frame, this
    ///   method should return `Ok(None)` and leave `src` in the same state.
    ///
    /// Returns
    /// - `Ok(Some(StompItem))` when a full item (frame or heartbeat) was
    ///   decoded and bytes were consumed from `src` accordingly.
    /// - `Ok(None)` when more bytes are required to decode a complete item.
    /// - `Err(io::Error)` on protocol or data errors (invalid UTF-8, malformed
    ///   frames, missing NUL after a content-length body, etc.).
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Move any newly-received bytes from the provided `src` into our
        // internal buffer. We keep a separate buffer so parsing can proceed
        // across arbitrary chunk boundaries without relying on indexes into
        // the caller-owned `src` (which may be mutated between calls).
        if !src.is_empty() {
            self.buf.extend_from_slice(&src[..]);
            src.clear();
        }

        let sep = b"\n\n";

        // Heartbeat (single LF)
        if !self.buf.is_empty() && self.buf[0] == b'\n' {
            // consume the heartbeat byte
            self.buf.split_to(1);
            return Ok(Some(StompItem::Heartbeat));
        }

        // Try to find the header/body separator. It's possible it isn't
        // present yet (stream is fragmented). We don't return early here:
        // even without the separator we can still decode a NUL-terminated
        // frame if a full NUL-terminated chunk is available (some servers
        // or split patterns may present a tiny frame that lacks a header
        // separator in the current chunk view).
        let sep_pos_opt = self.buf.windows(sep.len()).position(|w| w == sep);

        // find end of command (first LF) to compute header slice if we have sep
        let command_end = self.buf.iter().position(|&b| b == b'\n').unwrap_or(self.buf.len());
        let header_start = if command_end < self.buf.len() { command_end + 1 } else { command_end };

        // If we have a separator, parse headers and detect content-length.
        if let Some(sep_pos) = sep_pos_opt {
            let header_slice = if sep_pos > header_start { &self.buf[header_start..sep_pos] } else { &[][..] };
            let (headers, content_length) = parse_headers(header_slice)?;

            if let Some(clen) = content_length {
            // sep_pos + 2 (for "\n\n") + content + 1 (for trailing NUL)
            let needed = sep_pos + sep.len() + clen + 1;
            if self.buf.len() < needed {
                return Ok(None);
            }

            let nul_index = sep_pos + sep.len() + clen;
            if self.buf[nul_index] != 0 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "missing NUL after content-length body"));
            }

            // extract the full frame including the trailing NUL
            let frame_with_nul = self.buf.split_to(nul_index + 1);
            let raw = frame_with_nul[..nul_index].to_vec();

            if raw.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "empty frame"));
            }

            // recompute separator relative to raw to safely slice headers/body
            let sep_rel = raw.windows(sep.len()).position(|w| w == sep).ok_or_else(|| {
                let _ = eprintln!("missing header separator in frame; raw len={}", raw.len());
                io::Error::new(io::ErrorKind::InvalidData, "missing header separator in frame")
            })?;

            let cmd_end = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len());
            let command = String::from_utf8(raw[..cmd_end].to_vec()).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("invalid utf8 in command: {}", e))
            })?;

            let body_start = sep_rel + sep.len();
            let body = raw[body_start..].to_vec();

            let frame = Frame { command, headers, body };
            return Ok(Some(StompItem::Frame(frame)));
        }
        }

        // If we didn't have a separator, we may still have a complete
        // NUL-terminated frame available. Decode it permissively: use the
        // first NUL as a frame terminator, treat the bytes before the first
        // LF (if present) as the command, and treat headers as empty when
        // the separator is absent. This accepts tiny frames like
        // `"COMMAND\0"` (no headers) which can appear in split patterns.
        if let Some(nul_pos) = self.buf.iter().position(|&b| b == 0) {
            // consume up to and including the NUL
            let frame_with_nul = self.buf.split_to(nul_pos + 1);
            let raw = frame_with_nul[..nul_pos].to_vec();

            if raw.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "empty frame"));
            }

            let cmd_end = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len());
            let command = String::from_utf8(raw[..cmd_end].to_vec()).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("invalid utf8 in command: {}", e))
            })?;

            // If a separator exists within raw, use it to split headers/body,
            // otherwise assume no headers and empty body.
            let body = if let Some(sep_rel) = raw.windows(sep.len()).position(|w| w == sep) {
                let header_slice = if sep_rel > cmd_end + 1 { &raw[cmd_end + 1..sep_rel] } else { &[][..] };
                let (_headers, _cl) = parse_headers(header_slice)?;
                raw[sep_rel + sep.len()..].to_vec()
            } else {
                // no separator in raw -> no headers, body is empty
                Vec::new()
            };

            let headers = if let Some(sep_rel) = raw.windows(sep.len()).position(|w| w == sep) {
                if sep_rel > cmd_end + 1 { parse_headers(&raw[cmd_end + 1..sep_rel])?.0 } else { Vec::new() }
            } else { Vec::new() };

            let frame = Frame { command, headers, body };
            return Ok(Some(StompItem::Frame(frame)));
        }

        Ok(None)
    }
}

impl Encoder<StompItem> for StompCodec {
    type Error = io::Error;
    /// Encode a `StompItem` into the provided destination buffer.
    ///
    /// Parameters
    /// - `item`: the `StompItem` to encode. The encoder takes ownership of the
    ///   item (and any contained `Frame`) and may consume/mutate its contents.
    /// - `dst`: destination buffer where encoded bytes should be appended.
    ///   This is the same `BytesMut` provided by the `tokio_util::codec`
    ///   framework (e.g. `Framed`). Do not replace or reassign `dst`; instead
    ///   append bytes into it using `BufMut` methods (`put_u8`,
    ///   `put_slice`, `extend_from_slice`, etc.). After `encode` returns the
    ///   contents of `dst` will be written to the underlying transport.
    ///
    /// Returns
    /// - `Ok(())` on success, or `Err(io::Error)` on encoding-related errors.
    fn encode(&mut self, item: StompItem, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            StompItem::Heartbeat => {
                dst.put_u8(b'\n');
            }
            StompItem::Frame(frame) => {
                dst.extend_from_slice(frame.command.as_bytes());
                dst.put_u8(b'\n');

                let mut headers = frame.headers;
                let has_cl = headers
                    .iter()
                    .any(|(k, _)| k.to_lowercase() == "content-length");
                if !has_cl {
                    let include_cl =
                        frame.body.contains(&0) || std::str::from_utf8(&frame.body).is_err();
                    if include_cl {
                        headers.push(("content-length".to_string(), frame.body.len().to_string()));
                    }
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
