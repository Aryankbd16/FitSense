use std::io::{self, BufRead};

use serde::{Deserialize, Serialize};

pub const VERSION: u16 = 1;
pub const MAX_FRAME: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Health,
    Diagnostics,
    Shutdown,
}

#[derive(Serialize)]
pub struct Request<'a> {
    pub version: u16,
    pub id: &'a str,
    pub method: Method,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Frame {
    Ready {
        version: u16,
        service: String,
    },
    Response {
        version: u16,
        id: String,
        result: Payload,
    },
    Error {
        version: u16,
        id: Option<String>,
        code: ErrorCode,
    },
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    MalformedJson,
    InvalidRequest,
    UnsupportedVersion,
    UnknownMethod,
    InvalidFrame,
}

impl Frame {
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        serde_json::from_slice(bytes).map_err(|_| WireError::Protocol)
    }

    pub fn ready(self) -> Result<(), WireError> {
        match self {
            Frame::Ready {
                version: VERSION,
                service,
            } if service == "fitsense-worker" => Ok(()),
            _ => Err(WireError::Protocol),
        }
    }

    pub fn correlate(self, expected: &str) -> Result<Payload, WireError> {
        match self {
            Frame::Response {
                version: VERSION,
                id,
                result,
            } if id == expected => Ok(result),
            Frame::Error {
                version: VERSION,
                id: Some(id),
                code,
            } if id == expected => Err(WireError::Remote(code)),
            _ => Err(WireError::Protocol),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WireError {
    Protocol,
    Timeout,
    Exited,
    Remote(ErrorCode),
    Cancelled,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields, rename_all_fields = "camelCase")]
pub enum Payload {
    Status { status: WorkerState },
    Diagnostics { python_version: String },
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerState {
    Ok,
    Stopping,
}

/// Returns a line without LF or CR. Never buffers more than `limit` bytes.
pub fn read_frame(
    reader: &mut impl BufRead,
    limit: usize,
    discard_oversized: bool,
) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if line.is_empty() && !oversized {
                Ok(None)
            } else {
                Err(io::ErrorKind::InvalidData.into())
            };
        }
        let newline = available.iter().position(|&byte| byte == b'\n');
        let size = newline.map_or(available.len(), |index| index + 1);
        oversized |= line.len() + size > limit;
        if oversized && !discard_oversized {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if !oversized {
            line.extend_from_slice(&available[..size]);
        }
        reader.consume(size);
        if newline.is_some() {
            if oversized {
                line.clear();
                oversized = false;
                continue;
            }
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_request_serialization_and_response_correlation() {
        let request = Request {
            version: VERSION,
            id: "request-72",
            method: Method::Health,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"version":1,"id":"request-72","method":"health"}"#
        );
        let reply =
            br#"{"version":1,"type":"response","id":"request-72","result":{"status":"ok"}}"#;
        assert_eq!(
            Frame::decode(reply).unwrap().correlate("request-72"),
            Ok(Payload::Status {
                status: WorkerState::Ok
            })
        );
        assert_eq!(
            Frame::decode(reply).unwrap().correlate("request-73"),
            Err(WireError::Protocol)
        );
        let failure = br#"{"version":1,"type":"error","id":"request-73","code":"unknown_method"}"#;
        assert_eq!(
            Frame::decode(failure).unwrap().correlate("request-73"),
            Err(WireError::Remote(ErrorCode::UnknownMethod))
        );
        assert!(Frame::decode(b"arbitrary log output").is_err());
        assert!(Frame::decode(br#"{"version":1,"type":"response","id":"1","result":{"status":"stopping","status":"ok"}}"#).is_err());
        assert!(Frame::decode(br#"{"version":1,"type":"response","id":"1","result":{"status":"ok","pythonVersion":"3.13.17"}}"#).is_err());
        assert!(
            Frame::decode(br#"{"type":"ready","version":2,"service":"fitsense-worker"}"#)
                .unwrap()
                .ready()
                .is_err()
        );
    }

    #[test]
    fn frames_are_bounded_and_require_a_delimiter() {
        assert!(read_frame(&mut &b"123456789\n"[..], 8, false).is_err());
        assert!(read_frame(&mut &b"trunc"[..], 8, false).is_err());
        assert_eq!(
            read_frame(&mut &b"123456789\nokay\r\n"[..], 8, true)
                .unwrap()
                .unwrap(),
            b"okay"
        );
    }
}
