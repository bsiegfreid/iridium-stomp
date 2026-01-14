// Slice-based STOMP frame parser (produces owned Vecs from input slices)
/// Minimal helper: extract optional content-length header value from a header list.
///
/// Returns:
/// - Ok(Some(n)) when a valid Content-Length header is present and parsed.
/// - Ok(None) when no Content-Length header is present.
/// - Err(String) when Content-Length is present but not a valid unsigned integer.
type ParseResult =
    Result<Option<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>, Option<Vec<u8>>, usize)>, String>;

fn get_content_length(headers: &[(Vec<u8>, Vec<u8>)]) -> Result<Option<usize>, String> {
    for (k, v) in headers {
        if k.eq_ignore_ascii_case(&b"content-length"[..]) {
            let s =
                std::str::from_utf8(v).map_err(|e| format!("content-length not utf8: {}", e))?;
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Err("empty content-length".to_string());
            }
            match trimmed.parse::<usize>() {
                Ok(n) => return Ok(Some(n)),
                Err(e) => return Err(format!("invalid content-length '{}': {}", trimmed, e)),
            }
        }
    }
    Ok(None)
}

/// Parse a single STOMP frame from a raw byte slice.
///
/// Returns Ok(Some((command, headers, body, consumed_bytes))) when a full frame
/// was parsed and how many bytes were consumed. Returns Ok(None) when more
/// bytes are required. Returns Err on protocol errors.
pub fn parse_frame_slice(input: &[u8]) -> ParseResult {
    let mut pos = 0usize;
    let len = input.len();

    // skip any leading LF heartbeats
    while pos < len && input[pos] == b'\n' {
        // treat a single LF as a heartbeat frame (handled by caller if desired)
        // but we skip leading LFs here; the codec will detect heartbeat earlier
        pos += 1;
    }

    // parse command line: find next LF; if no LF, fall back to NUL-only frame
    let cmd_end_opt = input[pos..].iter().position(|&b| b == b'\n');
    let mut command: Vec<u8>;
    if let Some(cmd_end_rel) = cmd_end_opt {
        command = input[pos..pos + cmd_end_rel].to_vec();
        // strip trailing CR if present
        if command.last() == Some(&b'\r') {
            // remove trailing CR
            command.pop();
        }
        pos += cmd_end_rel + 1;
    } else {
        // No newline found: if there's a NUL in the remaining bytes, treat
        // this as a bare NUL-terminated body with empty command/headers.
        if let Some(nul_rel) = input[pos..].iter().position(|&b| b == 0) {
            let body = input[pos..pos + nul_rel].to_vec();
            pos += nul_rel + 1;
            if pos < len && input[pos] == b'\n' {
                pos += 1;
            }
            let body_opt = if body.is_empty() { None } else { Some(body) };
            return Ok(Some((Vec::new(), Vec::new(), body_opt, pos)));
        }
        return Ok(None);
    }

    // parse headers until an empty line (LF) is found
    let mut headers: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    loop {
        if pos >= len {
            return Ok(None);
        }
        if input[pos] == b'\n' {
            pos += 1; // consume blank line
            break;
        }
        // find end of header line
        let line_end_rel = match input[pos..].iter().position(|&b| b == b'\n') {
            Some(i) => i,
            None => return Ok(None),
        };
        let mut line = &input[pos..pos + line_end_rel];
        // strip trailing CR
        if !line.is_empty() && line[line.len() - 1] == b'\r' {
            line = &line[..line.len() - 1];
        }
        // find ':' separator
        if let Some(colon) = line.iter().position(|&b| b == b':') {
            let key = line[..colon].to_vec();
            let val = line[colon + 1..].to_vec();
            headers.push((key, val));
        } else {
            return Err(format!(
                "malformed header line: {:?}",
                String::from_utf8_lossy(line)
            ));
        }
        pos += line_end_rel + 1;
    }

    // determine body strategy
    match get_content_length(&headers) {
        Ok(Some(content_len)) => {
            // need content_len bytes, plus terminating NUL
            if pos + content_len + 1 > len {
                Ok(None)
            } else {
                let body = input[pos..pos + content_len].to_vec();
                pos += content_len;
                // next must be NUL
                if pos >= len || input[pos] != 0 {
                    Err("missing NUL terminator after content-length body".to_string())
                } else {
                    pos += 1;
                    // optional trailing LF
                    if pos < len && input[pos] == b'\n' {
                        pos += 1;
                    }
                    Ok(Some((command, headers, Some(body), pos)))
                }
            }
        }
        Ok(None) => {
            // NUL-terminated body: find NUL
            match input[pos..].iter().position(|&b| b == 0) {
                Some(nul_rel) => {
                    let body = input[pos..pos + nul_rel].to_vec();
                    pos += nul_rel + 1;
                    // optional trailing LF
                    if pos < len && input[pos] == b'\n' {
                        pos += 1;
                    }
                    let body_opt = if body.is_empty() { None } else { Some(body) };
                    Ok(Some((command, headers, body_opt, pos)))
                }
                None => Ok(None),
            }
        }
        Err(e) => Err(e),
    }
}
