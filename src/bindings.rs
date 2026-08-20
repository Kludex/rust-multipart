use pyo3::{prelude::*, types::PyBytes};

use crate::{
    form_data::FormData,
    multipart::{MultipartParser, MultipartPart, MultipartState},
};

#[derive(Debug, Clone, FromPyObject)]
pub struct BytesWrapper(Vec<u8>);

impl IntoPy<PyObject> for BytesWrapper {
    fn into_py(self, py: Python<'_>) -> PyObject {
        PyBytes::new_bound(py, &self.0).into_any().unbind()
    }
}

#[pyclass(name = "MultipartState", eq, eq_int)]
#[derive(Clone, PartialEq, Debug)]
pub enum PyMultipartState {
    #[pyo3(name = "PREAMBLE")]
    Preamble,
    #[pyo3(name = "HEADER")]
    Header,
    #[pyo3(name = "BODY")]
    Body,
    #[pyo3(name = "END")]
    End,
}

impl From<MultipartState> for PyMultipartState {
    fn from(state: MultipartState) -> Self {
        match state {
            MultipartState::Preamble => Self::Preamble,
            MultipartState::Header => Self::Header,
            MultipartState::Body => Self::Body,
            MultipartState::End => Self::End,
        }
    }
}

#[pyclass(name = "MultipartPart")]
#[derive(Debug, Clone)]
pub enum PyMultipartPart {
    Header { name: String, value: String },
    Body { data: BytesWrapper, complete: bool },
}

impl From<MultipartPart> for PyMultipartPart {
    fn from(part: MultipartPart) -> Self {
        match part {
            MultipartPart::Header { name, value } => Self::Header { name, value },
            MultipartPart::Body { data, complete } => Self::Body {
                data: BytesWrapper(data),
                complete,
            },
        }
    }
}

#[pymethods]
impl PyMultipartPart {
    fn __repr__(&self) -> String {
        match self {
            Self::Header { name, value } => format!("Header(name={name:?}, value={value:?})"),
            Self::Body { data, complete } => format!("Body(data={:?}, complete={complete})", data.0),
        }
    }
}

#[pyclass(name = "FormData")]
#[derive(Debug, Clone)]
pub enum PyFormData {
    Field {
        name: String,
        content_type: String,
        charset: String,
        data: BytesWrapper,
    },
    File {
        name: String,
        filename: String,
        content_type: String,
        charset: String,
        data: BytesWrapper,
    },
}

impl From<FormData> for PyFormData {
    fn from(part: FormData) -> Self {
        match part {
            FormData::Field {
                name,
                content_type,
                charset,
                data,
            } => Self::Field {
                name,
                content_type,
                charset,
                data: BytesWrapper(data),
            },
            FormData::File {
                name,
                filename,
                content_type,
                charset,
                data,
            } => Self::File {
                name,
                filename,
                content_type,
                charset,
                data: BytesWrapper(data),
            },
        }
    }
}

#[pyclass(name = "MultipartParser")]
pub struct PyMultipartParser {
    parser: MultipartParser,
}

#[pymethods]
impl PyMultipartParser {
    #[new]
    #[pyo3(signature = (boundary, max_size = None, header_charset = "utf8"))]
    fn new(boundary: Vec<u8>, max_size: Option<usize>, header_charset: &str) -> PyResult<Self> {
        Ok(Self {
            parser: MultipartParser::new(boundary, max_size, header_charset)?,
        })
    }

    #[getter]
    fn state(&self) -> PyMultipartState {
        self.parser.state().into()
    }

    fn parse(&mut self, data: Vec<u8>) -> PyResult<()> {
        self.parser.feed(data)
    }

    fn next_part(&mut self) -> Option<PyFormData> {
        self.parser.next_part().map(Into::into)
    }

    fn next_event(&mut self) -> Option<PyMultipartPart> {
        self.parser.next_event().map(Into::into)
    }
}
