//! Sans-IO parser for [RFC 7578](https://www.rfc-editor.org/rfc/rfc7578.html) `multipart/form-data`.
//!
//! The parser transitions from the preamble to part headers, then to the part body. A body can start another part or
//! finish the multipart message.

use std::collections::{HashMap, VecDeque};
use std::str;

use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
};

use crate::form_data::FormData;

const CRLF: &[u8] = b"\r\n";

#[derive(Clone, PartialEq, Debug)]
pub enum MultipartState {
    Preamble,
    Header,
    Body,
    End,
}

#[derive(Debug, Clone)]
pub enum MultipartPart {
    Header { name: String, value: String },
    Body { data: Vec<u8>, complete: bool },
}

impl MultipartPart {
    fn build_header(data: &[u8]) -> PyResult<Self> {
        let separator = data
            .iter()
            .position(|character| *character == b':')
            .ok_or_else(|| PyValueError::new_err("Malformed header"))?;
        let name = str::from_utf8(&data[..separator])
            .map_err(|_| PyValueError::new_err("Invalid header name"))?
            .trim();
        let value = str::from_utf8(&data[separator + 1..])
            .map_err(|_| PyValueError::new_err("Invalid header value"))?
            .trim();
        if name.is_empty() {
            return Err(PyValueError::new_err("Missing header name"));
        }
        Ok(Self::Header {
            name: name.to_ascii_lowercase(),
            value: value.to_string(),
        })
    }
}

#[derive(Debug, PartialEq)]
enum DelimiterSuffix {
    Open(usize),
    Close,
    Incomplete,
    Invalid,
    BareLineFeed,
}

pub struct MultipartParser {
    max_size: Option<usize>,
    state: MultipartState,
    buffer: Vec<u8>,
    dash_boundary: Vec<u8>,
    delimiter: Vec<u8>,
    size: usize,
    events: VecDeque<MultipartPart>,
    current_headers: HashMap<String, String>,
    current_part: Option<FormData>,
    parts: VecDeque<FormData>,
}

impl MultipartParser {
    pub fn new(boundary: Vec<u8>, max_size: Option<usize>, header_charset: &str) -> PyResult<Self> {
        // RFC 2046 section 5.1.1 limits boundary values to 70 characters.
        if boundary.is_empty() || boundary.len() > 70 {
            return Err(PyValueError::new_err("Boundary length must be between 1 and 70 characters."));
        }
        if header_charset != "utf8" {
            return Err(PyRuntimeError::new_err("The only supported charset is 'utf8'."));
        }

        let dash_boundary = [b"--".as_slice(), &boundary].concat();
        let delimiter = [CRLF, dash_boundary.as_slice()].concat();
        Ok(Self {
            max_size,
            state: MultipartState::Preamble,
            buffer: Vec::new(),
            dash_boundary,
            delimiter,
            size: 0,
            events: VecDeque::new(),
            current_headers: HashMap::new(),
            current_part: None,
            parts: VecDeque::new(),
        })
    }

    pub fn state(&self) -> MultipartState {
        self.state.clone()
    }

    pub fn feed(&mut self, data: Vec<u8>) -> PyResult<()> {
        if self.state == MultipartState::End {
            return Ok(());
        }
        if self.max_size.is_some_and(|max_size| self.size.saturating_add(data.len()) > max_size) {
            return Err(PyRuntimeError::new_err("Data exceeds maximum size."));
        }
        self.size += data.len();
        self.buffer.extend(data);

        loop {
            let progressed = match self.state {
                MultipartState::Preamble => self.handle_preamble()?,
                MultipartState::Header => self.handle_header()?,
                MultipartState::Body => self.handle_body()?,
                MultipartState::End => false,
            };
            if !progressed {
                return Ok(());
            }
        }
    }

    pub fn next_part(&mut self) -> Option<FormData> {
        self.parts.pop_front()
    }

    pub fn next_event(&mut self) -> Option<MultipartPart> {
        self.events.pop_front()
    }

    fn handle_preamble(&mut self) -> PyResult<bool> {
        // RFC 2046 section 5.1.1 requires recipients to ignore text before the first boundary delimiter.
        let mut search_from = 0;
        while let Some(relative_index) = find_subslice(&self.buffer[search_from..], &self.dash_boundary) {
            let index = search_from + relative_index;
            let starts_line = index == 0 || (index >= 2 && self.buffer[index - 2..index] == *CRLF);
            if !starts_line {
                search_from = index + 1;
                continue;
            }

            match delimiter_suffix(&self.buffer, index + self.dash_boundary.len()) {
                DelimiterSuffix::Open(consumed) => {
                    self.buffer.drain(..consumed);
                    self.state = MultipartState::Header;
                    return Ok(true);
                }
                DelimiterSuffix::Close => {
                    self.buffer.clear();
                    self.state = MultipartState::End;
                    return Ok(true);
                }
                DelimiterSuffix::Incomplete => {
                    self.buffer.drain(..index);
                    return Ok(false);
                }
                DelimiterSuffix::Invalid => search_from = index + 1,
                DelimiterSuffix::BareLineFeed => {
                    return Err(PyValueError::new_err("Invalid line break after delimiter"));
                }
            }
        }

        let retained = self.dash_boundary.len() + 1;
        if self.buffer.len() > retained {
            self.buffer.drain(..self.buffer.len() - retained);
        }
        Ok(false)
    }

    fn handle_header(&mut self) -> PyResult<bool> {
        if let Some(index) = find_subslice(&self.buffer, CRLF) {
            let line = self.buffer[..index].to_vec();
            self.buffer.drain(..index + CRLF.len());
            if line.is_empty() {
                self.current_part = Some(FormData::try_from(std::mem::take(&mut self.current_headers))?);
                self.state = MultipartState::Body;
            } else if let MultipartPart::Header { name, value } = MultipartPart::build_header(&line)? {
                self.current_headers.insert(name.clone(), value.clone());
                self.events.push_back(MultipartPart::Header { name, value });
            }
            return Ok(true);
        }
        if self.buffer.contains(&b'\n') {
            return Err(PyValueError::new_err("Invalid line break in header"));
        }
        Ok(false)
    }

    fn handle_body(&mut self) -> PyResult<bool> {
        let mut search_from = 0;
        while let Some(relative_index) = find_subslice(&self.buffer[search_from..], &self.delimiter) {
            let index = search_from + relative_index;
            match delimiter_suffix(&self.buffer, index + self.delimiter.len()) {
                DelimiterSuffix::Open(consumed) => {
                    let data = self.buffer[..index].to_vec();
                    self.finish_part(&data);
                    self.buffer.drain(..consumed);
                    self.state = MultipartState::Header;
                    return Ok(true);
                }
                DelimiterSuffix::Close => {
                    let data = self.buffer[..index].to_vec();
                    self.finish_part(&data);
                    self.buffer.clear();
                    self.state = MultipartState::End;
                    return Ok(true);
                }
                DelimiterSuffix::Incomplete => {
                    let data = self.buffer[..index].to_vec();
                    self.append_body(&data, false);
                    self.buffer.drain(..index);
                    return Ok(false);
                }
                DelimiterSuffix::Invalid => search_from = index + CRLF.len(),
                DelimiterSuffix::BareLineFeed => {
                    return Err(PyValueError::new_err("Invalid line break after delimiter"));
                }
            }
        }

        let retained = self.delimiter.len() - 1;
        if self.buffer.len() > retained {
            let emitted = self.buffer.len() - retained;
            let data = self.buffer[..emitted].to_vec();
            self.append_body(&data, false);
            self.buffer.drain(..emitted);
        }
        Ok(false)
    }

    fn append_body(&mut self, data: &[u8], complete: bool) {
        let part = self.current_part.as_mut().expect("body state requires a current part");
        part.append_data(data);
        if !data.is_empty() || complete {
            self.events.push_back(MultipartPart::Body { data: data.to_vec(), complete });
        }
    }

    fn finish_part(&mut self, data: &[u8]) {
        self.append_body(data, true);
        self.parts.push_back(self.current_part.take().expect("body state requires a current part"));
    }
}

fn delimiter_suffix(buffer: &[u8], after_boundary: usize) -> DelimiterSuffix {
    let tail = &buffer[after_boundary..];
    if tail.is_empty() {
        return DelimiterSuffix::Incomplete;
    }
    if tail.starts_with(b"--") {
        return DelimiterSuffix::Close;
    }
    if tail[0] == b'-' {
        return if tail.len() == 1 {
            DelimiterSuffix::Incomplete
        } else {
            DelimiterSuffix::Invalid
        };
    }

    // RFC 2046 section 5.1.1 permits linear whitespace between the boundary and CRLF.
    let padding = tail.iter().take_while(|byte| matches!(byte, b' ' | b'\t')).count();
    let line_ending = &tail[padding..];
    if line_ending.is_empty() || line_ending == b"\r" {
        return DelimiterSuffix::Incomplete;
    }
    if line_ending.starts_with(CRLF) {
        return DelimiterSuffix::Open(after_boundary + padding + CRLF.len());
    }
    if line_ending[0] == b'\n' {
        return DelimiterSuffix::BareLineFeed;
    }
    DelimiterSuffix::Invalid
}

fn find_subslice(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|window| window == needle)
}
