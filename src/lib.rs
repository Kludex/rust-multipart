use pyo3::prelude::*;

mod bindings;
mod form_data;
mod headers;
mod multipart;

#[pymodule]
fn parser(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<bindings::PyMultipartParser>()?;
    module.add_class::<bindings::PyMultipartState>()?;
    module.add_class::<bindings::PyMultipartPart>()?;
    module.add_class::<bindings::PyFormData>()?;
    Ok(())
}
