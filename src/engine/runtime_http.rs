//! Small bounded HTTP reader for the pinned loopback observation endpoints.
//! No DNS, proxies, redirects, credentials or general-purpose request surface.
use super::fail;
use crate::error::Result;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

pub(super) const MEDIA: &str = "application/vnd.nigo.console+json";
const LIMIT: usize = 262_144;
const HEADER_LIMIT: usize = 16_384;
pub(super) struct Response {
    pub body: Vec<u8>,
    pub media: String,
}

pub(super) fn get(address: SocketAddr, endpoint: &str, timeout: Duration) -> Result<Response> {
    if !address.ip().is_loopback()
        || address.port() == 0
        || timeout.is_zero()
        || !matches!(
            endpoint,
            "/monitor/api/console/bootstrap" | "/monitor/api/consensus/health"
        )
    {
        return Err(invalid());
    }
    let deadline = Instant::now() + timeout.min(Duration::from_secs(10));
    let mut stream =
        TcpStream::connect_timeout(&address, remaining(deadline)?).map_err(|_| invalid())?;
    stream
        .set_write_timeout(Some(remaining(deadline)?))
        .map_err(|_| invalid())?;
    let request = format!(
        "GET {endpoint} HTTP/1.1\r\nHost: {address}\r\nAccept: {MEDIA}\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| invalid())?;
    let mut raw = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(|_| invalid())?;
        let count = stream.read(&mut buffer).map_err(|_| invalid())?;
        if count == 0 {
            break;
        }
        if raw.len() + count > LIMIT + HEADER_LIMIT {
            return Err(invalid());
        }
        raw.extend_from_slice(&buffer[..count]);
        if !raw.windows(4).any(|v| v == b"\r\n\r\n") && raw.len() > HEADER_LIMIT {
            return Err(invalid());
        }
    }
    parse(&raw)
}
fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(invalid)
}
fn parse(raw: &[u8]) -> Result<Response> {
    let split = raw
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .ok_or_else(invalid)?;
    if split > HEADER_LIMIT || raw.len() > LIMIT + HEADER_LIMIT {
        return Err(invalid());
    }
    let headers = std::str::from_utf8(&raw[..split]).map_err(|_| invalid())?;
    let mut lines = headers.split("\r\n");
    let status = lines.next().ok_or_else(invalid)?;
    if !status.starts_with("HTTP/1.1 200 ") && !status.starts_with("HTTP/1.0 200 ") {
        return Err(invalid());
    }
    let mut length = None;
    let mut chunked = false;
    let mut media = None;
    for line in lines {
        let (key, value) = line.split_once(':').ok_or_else(invalid)?;
        let value = value.trim();
        match key.to_ascii_lowercase().as_str() {
            "content-length" => {
                if length.is_some() || !value.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(invalid());
                }
                length = Some(value.parse::<usize>().map_err(|_| invalid())?);
            }
            "transfer-encoding" => {
                if chunked || !value.eq_ignore_ascii_case("chunked") {
                    return Err(invalid());
                }
                chunked = true;
            }
            "content-type" => {
                if media.is_some() {
                    return Err(invalid());
                }
                media = Some(value.to_owned());
            }
            "content-encoding" if !value.eq_ignore_ascii_case("identity") => return Err(invalid()),
            _ => (),
        }
    }
    let payload = &raw[split + 4..];
    if (chunked && length.is_some()) || payload.len() > LIMIT {
        return Err(invalid());
    }
    let body = if chunked {
        chunks(payload)?
    } else {
        if length.is_some_and(|n| n != payload.len()) {
            return Err(invalid());
        }
        payload.to_vec()
    };
    let media = media.ok_or_else(invalid)?;
    if body.is_empty() || media.split(';').next().map(str::trim) != Some(MEDIA) {
        return Err(invalid());
    }
    Ok(Response { body, media })
}
fn chunks(mut bytes: &[u8]) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let end = bytes
            .windows(2)
            .position(|v| v == b"\r\n")
            .ok_or_else(invalid)?;
        let size = &bytes[..end];
        if size.is_empty() || size.len() > 8 || !size.iter().all(u8::is_ascii_hexdigit) {
            return Err(invalid());
        }
        let size = usize::from_str_radix(std::str::from_utf8(size).map_err(|_| invalid())?, 16)
            .map_err(|_| invalid())?;
        bytes = &bytes[end + 2..];
        if size == 0 {
            if bytes != b"\r\n" {
                return Err(invalid());
            }
            return Ok(body);
        }
        if size > LIMIT - body.len() || bytes.len() < size + 2 || &bytes[size..size + 2] != b"\r\n"
        {
            return Err(invalid());
        }
        body.extend_from_slice(&bytes[..size]);
        bytes = &bytes[size + 2..];
    }
}
fn invalid() -> crate::error::BxdlError {
    fail(
        "SERVICE_HTTP_UNKNOWN",
        "로컬 엔진의 제한된 관측 응답을 확인하지 못했습니다. 정상·정지·재시작 가능으로 간주하지 않습니다.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_redirects_truncation_ambiguous_framing_and_wrong_media() {
        let prefix = format!("HTTP/1.1 200 OK\r\nContent-Type: {MEDIA}\r\n");
        assert_eq!(
            parse(format!("{prefix}Content-Length: 2\r\n\r\n{{}}").as_bytes())
                .unwrap()
                .body,
            b"{}"
        );
        for suffix in [
            "Content-Length: 3\r\n\r\n{}",
            "Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
            "Content-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n{}",
            "Content-Encoding: gzip\r\n\r\n{}",
            "\r\n",
        ] {
            assert!(parse(format!("{prefix}{suffix}").as_bytes()).is_err());
        }
        assert!(parse(b"HTTP/1.1 302 Found\r\nLocation: http://example.test\r\n\r\n{}").is_err());
        assert!(parse(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}").is_err());
    }
    #[test]
    fn chunking_is_bounded_and_requires_complete_terminator() {
        assert_eq!(chunks(b"1\r\n{\r\n1\r\n}\r\n0\r\n\r\n").unwrap(), b"{}");
        for raw in [
            b"2\r\n{}\r\n0\r\n".as_slice(),
            b"fffffff\r\n",
            b"0\r\n\r\ntrailing",
            b"2\r\n{\r\n",
        ] {
            assert!(chunks(raw).is_err());
        }
    }
    #[test]
    fn never_connects_to_remote_or_arbitrary_endpoint() {
        assert!(
            get(
                "192.0.2.1:80".parse().unwrap(),
                "/monitor/api/consensus/health",
                Duration::from_millis(1)
            )
            .is_err()
        );
        assert!(
            get(
                "127.0.0.1:80".parse().unwrap(),
                "/admin/stop",
                Duration::from_millis(1)
            )
            .is_err()
        );
    }
    #[test]
    fn consumes_actual_bounded_loopback_response() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 1024];
            let count = stream.read(&mut request).unwrap();
            assert!(
                std::str::from_utf8(&request[..count])
                    .unwrap()
                    .contains(MEDIA)
            );
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: {MEDIA}\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{{}}\r\n0\r\n\r\n").unwrap();
        });
        assert_eq!(
            get(
                address,
                "/monitor/api/consensus/health",
                Duration::from_secs(2)
            )
            .unwrap()
            .body,
            b"{}"
        );
        server.join().unwrap();
    }
}
