//! Builder for [RFC 7578](https://www.rfc-editor.org/rfc/rfc7578.html) `multipart/form-data` bodies.
//!
//! The builder is the inverse of the parser and works at two levels. The eager level (`add_field`, `add_file`,
//! `add_part`, `build`) accumulates a complete body. The streaming level (`render_field`, `render_file`,
//! `render_part`, `closing`) returns each part's prologue as bytes so callers can interleave file data from
//! disk without buffering it - the shape HTTP clients like httpx need for chunked uploads.
//!
//! Names and filenames are escaped with the WHATWG HTML5 form rules (`"` and C0 controls to `%XX`, `\` doubled),
//! matching how browsers and httpx serialize form submissions.

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
            '\\' => escaped.push_str("\\\\"),
            '\x1b' => escaped.push(character),
            control if (control as u32) <= 0x1f => {
                escaped.push_str(&format!("%{:02X}", control as u32));
            }
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

    fn disposition(name: &str, filename: Option<&str>) -> Vec<u8> {
        let mut disposition = format!("form-data; name=\"{}\"", escape(name)).into_bytes();
        if let Some(filename) = filename {
            disposition.extend_from_slice(format!("; filename=\"{}\"", escape(filename)).as_bytes());
        }
        disposition
    }

    pub fn render_part(&self, headers: &[(Vec<u8>, Vec<u8>)]) -> PyResult<Vec<u8>> {
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
        let mut prologue = Vec::new();
        prologue.extend_from_slice(b"--");
        prologue.extend_from_slice(&self.boundary);
        prologue.extend_from_slice(CRLF);
        for (name, value) in headers {
            prologue.extend_from_slice(name);
            prologue.extend_from_slice(b": ");
            prologue.extend_from_slice(value);
            prologue.extend_from_slice(CRLF);
        }
        prologue.extend_from_slice(CRLF);
        Ok(prologue)
    }

    pub fn render_field(&self, name: &str) -> PyResult<Vec<u8>> {
        self.render_part(&[(b"Content-Disposition".to_vec(), Self::disposition(name, None))])
    }

    pub fn render_file(&self, name: &str, filename: &str, content_type: Option<&str>) -> PyResult<Vec<u8>> {
        let mut headers = vec![(b"Content-Disposition".to_vec(), Self::disposition(name, Some(filename)))];
        if let Some(content_type) = content_type {
            headers.push((b"Content-Type".to_vec(), content_type.as_bytes().to_vec()));
        }
        self.render_part(&headers)
    }

    pub fn closing(&self) -> Vec<u8> {
        let mut closing = Vec::with_capacity(self.boundary.len() + 6);
        closing.extend_from_slice(b"--");
        closing.extend_from_slice(&self.boundary);
        closing.extend_from_slice(b"--");
        closing.extend_from_slice(CRLF);
        closing
    }

    fn append(&mut self, prologue: Vec<u8>, data: &[u8]) -> PyResult<()> {
        if self.finished {
            return Err(PyRuntimeError::new_err("Builder already finished."));
        }
        self.buffer.extend_from_slice(&prologue);
        self.buffer.extend_from_slice(data);
        self.buffer.extend_from_slice(CRLF);
        Ok(())
    }

    pub fn add_part(&mut self, headers: &[(Vec<u8>, Vec<u8>)], data: &[u8]) -> PyResult<()> {
        let prologue = self.render_part(headers)?;
        self.append(prologue, data)
    }

    pub fn add_field(&mut self, name: &str, value: &[u8]) -> PyResult<()> {
        let prologue = self.render_field(name)?;
        self.append(prologue, value)
    }

    pub fn add_file(&mut self, name: &str, filename: &str, data: &[u8], content_type: Option<&str>) -> PyResult<()> {
        let prologue = self.render_file(name, filename, content_type)?;
        self.append(prologue, data)
    }

    pub fn build(&mut self) -> PyResult<Vec<u8>> {
        if self.finished {
            return Err(PyRuntimeError::new_err("Builder already finished."));
        }
        self.finished = true;
        self.buffer.extend_from_slice(&self.closing());
        Ok(std::mem::take(&mut self.buffer))
    }
}
