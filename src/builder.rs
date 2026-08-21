//! Builder for [RFC 7578](https://www.rfc-editor.org/rfc/rfc7578.html) `multipart/form-data` bodies.
//!
//! The builder is the inverse of the parser: it serializes parts into a body an HTTP client can send. `add_part()`
//! takes raw headers, mirroring the parser's raw events; `add_field()` and `add_file()` render `Content-Disposition`
//! with WHATWG-style percent escaping for names and filenames.

use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
};

const CRLF: &[u8] = b"\r\n";
const MAX_BOUNDARY_SIZE: usize = 70;

fn is_valid_boundary_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'\'' | b'(' | b')' | b'+' | b'_' | b',' | b'-' | b'.' | b'/' | b':' | b'=' | b'?' | b' ')
}

fn random_boundary() -> Vec<u8> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS randomness is available");
    let mut boundary = Vec::with_capacity(32);
    for byte in bytes {
        boundary.push(char::from_digit((byte >> 4) as u32, 16).unwrap() as u8);
        boundary.push(char::from_digit((byte & 0xf) as u32, 16).unwrap() as u8);
    }
    boundary
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("%22"),
            '\r' => escaped.push_str("%0D"),
            '\n' => escaped.push_str("%0A"),
            _ => escaped.push(character),
        }
    }
    escaped
}

pub struct MultipartBuilder {
    boundary: Vec<u8>,
    buffer: Vec<u8>,
    finished: bool,
}

impl MultipartBuilder {
    pub fn new(boundary: Option<Vec<u8>>) -> PyResult<Self> {
        let boundary = boundary.unwrap_or_else(random_boundary);
        if boundary.is_empty() || boundary.len() > MAX_BOUNDARY_SIZE {
            return Err(PyValueError::new_err("Boundary must be between 1 and 70 bytes long"));
        }
        if !boundary.iter().all(|&byte| is_valid_boundary_byte(byte)) || boundary.ends_with(b" ") {
            return Err(PyValueError::new_err("Boundary contains invalid characters"));
        }
        Ok(Self {
            boundary,
            buffer: Vec::new(),
            finished: false,
        })
    }

    pub fn boundary(&self) -> &[u8] {
        &self.boundary
    }

    pub fn content_type(&self) -> String {
        let boundary = std::str::from_utf8(&self.boundary).expect("boundary bytes are validated ASCII");
        if boundary.contains(' ') {
            format!("multipart/form-data; boundary=\"{boundary}\"")
        } else {
            format!("multipart/form-data; boundary={boundary}")
        }
    }

    pub fn add_part(&mut self, headers: &[(Vec<u8>, Vec<u8>)], data: &[u8]) -> PyResult<()> {
        if self.finished {
            return Err(PyRuntimeError::new_err("Builder already finished."));
        }
        for (name, value) in headers {
            if name.is_empty() {
                return Err(PyValueError::new_err("Missing header name"));
            }
            if memchr::memchr3(b':', b'\r', b'\n', name).is_some() {
                return Err(PyValueError::new_err("Invalid character in header name"));
            }
            if memchr::memchr2(b'\r', b'\n', value).is_some() {
                return Err(PyValueError::new_err("Invalid line break in header value"));
            }
        }
        self.buffer.extend_from_slice(b"--");
        self.buffer.extend_from_slice(&self.boundary);
        self.buffer.extend_from_slice(CRLF);
        for (name, value) in headers {
            self.buffer.extend_from_slice(name);
            self.buffer.extend_from_slice(b": ");
            self.buffer.extend_from_slice(value);
            self.buffer.extend_from_slice(CRLF);
        }
        self.buffer.extend_from_slice(CRLF);
        self.buffer.extend_from_slice(data);
        self.buffer.extend_from_slice(CRLF);
        Ok(())
    }

    pub fn add_field(&mut self, name: &str, value: &[u8]) -> PyResult<()> {
        let disposition = format!("form-data; name=\"{}\"", escape(name));
        self.add_part(&[(b"Content-Disposition".to_vec(), disposition.into_bytes())], value)
    }

    pub fn add_file(&mut self, name: &str, filename: &str, data: &[u8], content_type: Option<&str>) -> PyResult<()> {
        let disposition = format!("form-data; name=\"{}\"; filename=\"{}\"", escape(name), escape(filename));
        let mut headers = vec![(b"Content-Disposition".to_vec(), disposition.into_bytes())];
        if let Some(content_type) = content_type {
            headers.push((b"Content-Type".to_vec(), content_type.as_bytes().to_vec()));
        }
        self.add_part(&headers, data)
    }

    pub fn build(&mut self) -> PyResult<Vec<u8>> {
        if self.finished {
            return Err(PyRuntimeError::new_err("Builder already finished."));
        }
        self.finished = true;
        self.buffer.extend_from_slice(b"--");
        self.buffer.extend_from_slice(&self.boundary);
        self.buffer.extend_from_slice(b"--");
        self.buffer.extend_from_slice(CRLF);
        Ok(std::mem::take(&mut self.buffer))
    }
}
