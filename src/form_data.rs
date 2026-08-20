use std::collections::HashMap;

use pyo3::{exceptions::PyValueError, prelude::*};

use crate::headers::parse_options_header;

#[derive(Debug, Clone)]
pub enum FormData {
    Field {
        name: String,
        content_type: String,
        charset: String,
        data: Vec<u8>,
    },
    File {
        name: String,
        filename: String,
        content_type: String,
        charset: String,
        data: Vec<u8>,
    },
}

impl FormData {
    pub fn append_data(&mut self, data: &[u8]) {
        match self {
            Self::Field { data: field_data, .. } | Self::File { data: field_data, .. } => {
                field_data.extend_from_slice(data);
            }
        }
    }
}

impl TryFrom<HashMap<String, String>> for FormData {
    type Error = PyErr;

    fn try_from(headers: HashMap<String, String>) -> PyResult<Self> {
        let (content_type, content_type_parameters) = match headers.get("content-type") {
            Some(value) => parse_options_header(value).map_err(PyValueError::new_err)?,
            None => ("text/plain".to_string(), HashMap::new()),
        };
        let charset = content_type_parameters.get("charset").cloned().unwrap_or_else(|| "utf-8".to_string());

        let content_disposition = headers
            .get("content-disposition")
            .ok_or_else(|| PyValueError::new_err("Missing content-disposition header"))?;
        let (disposition, disposition_parameters) = parse_options_header(content_disposition).map_err(PyValueError::new_err)?;
        if !disposition.eq_ignore_ascii_case("form-data") {
            return Err(PyValueError::new_err("Invalid content-disposition"));
        }
        let name = disposition_parameters
            .get("name")
            .cloned()
            .ok_or_else(|| PyValueError::new_err("Parameter 'name' not found in content-disposition."))?;

        match disposition_parameters.get("filename") {
            Some(filename) => Ok(Self::File {
                name,
                filename: filename.clone(),
                content_type,
                charset,
                data: Vec::new(),
            }),
            None => Ok(Self::Field {
                name,
                content_type,
                charset,
                data: Vec::new(),
            }),
        }
    }
}
