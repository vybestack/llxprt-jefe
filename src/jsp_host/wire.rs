//! HTTP/1.1 request parsing and response writing for the JSP host.
//!
//! This is the wire boundary only: it parses an authenticated request off a
//! `TcpStream` and writes a status-only response. It holds no protocol state
//! and performs no authorization decisions.

use std::io::{Read, Write};
use std::net::TcpStream;

use super::{JspHostError, MAX_REQUEST_BYTES};

#[derive(Debug)]
pub(super) struct Request {
    pub(super) route: String,
    pub(super) token: String,
    pub(super) registration_id: String,
    pub(super) body: Vec<u8>,
}

pub(super) fn read_request(stream: &mut TcpStream) -> Result<Request, JspHostError> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let header_end = loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 || bytes.len().saturating_add(read) > MAX_REQUEST_BYTES {
            return Err(JspHostError::InvalidRequest);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(end) = find_header_end(&bytes) {
            break end;
        }
    };
    let header =
        std::str::from_utf8(&bytes[..header_end]).map_err(|_| JspHostError::InvalidRequest)?;
    let mut lines = header.split("\r\n");
    let request_line = lines.next().ok_or(JspHostError::InvalidRequest)?;
    let mut parts = request_line.split(' ');
    let method = parts.next();
    let route = parts.next();
    let version = parts.next();
    if method != Some("POST") || version != Some("HTTP/1.1") || parts.next().is_some() {
        return Err(JspHostError::InvalidRequest);
    }
    let route = route.ok_or(JspHostError::InvalidRequest)?.to_string();
    let mut token = None;
    let mut registration_id = None;
    let mut content_length = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(JspHostError::InvalidRequest)?;
        match name.to_ascii_lowercase().as_str() {
            "authorization" => token = value.trim().strip_prefix("Bearer ").map(str::to_string),
            "jsp-registration-id" => registration_id = Some(value.trim().to_string()),
            "content-length" => content_length = value.trim().parse::<usize>().ok(),
            _ => {}
        }
    }
    let content_length = content_length.ok_or(JspHostError::InvalidRequest)?;
    if content_length > MAX_REQUEST_BYTES {
        return Err(JspHostError::InvalidRequest);
    }
    let body_start = header_end.saturating_add(4);
    while bytes.len().saturating_sub(body_start) < content_length {
        let read = stream.read(&mut buffer)?;
        if read == 0 || bytes.len().saturating_add(read) > MAX_REQUEST_BYTES {
            return Err(JspHostError::InvalidRequest);
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(Request {
        route,
        token: token.ok_or(JspHostError::Unauthorized)?,
        registration_id: registration_id.ok_or(JspHostError::Forbidden)?,
        body: bytes[body_start..body_start + content_length].to_vec(),
    })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

pub(super) fn write_response(stream: &mut TcpStream, status: u16) -> Result<(), JspHostError> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        409 => "Conflict",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()?;
    Ok(())
}
