# rust-multipart

> [!WARNING]
> This project is a work in progress and is not ready for production use.

This package provides a Sans-IO parser for [RFC 7578](https://datatracker.ietf.org/doc/html/rfc7578) `multipart/form-data`.
It is heavily inspired by [defnull/multipart](https://github.com/defnull/multipart).

## Installation

```bash
pip install rust-multipart
```

## Usage

```python
from rust_multipart import MultipartParser, PartBegin, PartData, PartEnd

parser = MultipartParser(boundary=b"boundary")
events = parser.feed(b'--boundary\r\nContent-Disposition: form-data; name="user"\r\n\r\nPotato\r\n--boundary--\r\n')
parser.finish()

begin, data, end = events
assert isinstance(begin, PartBegin)
assert begin.headers == [(b"Content-Disposition", b'form-data; name="user"')]
assert isinstance(data, PartData)
assert data.data == b"Potato"
assert isinstance(end, PartEnd)
```

You can call `feed()` repeatedly with partial input. Each call returns the batch of events produced by that chunk:
`PartBegin` carries the ordered raw byte headers, `PartData` carries a body chunk, and `PartEnd` closes the part. The
parser never accumulates part bodies, so memory stays bounded regardless of upload size. Call `finish()` once the
input ends: it raises `ValueError` if the closing boundary was never received.

Use `parse_options_header()` to parse header values like `Content-Disposition`:

```python
from rust_multipart import parse_options_header

value, parameters = parse_options_header('form-data; name="user"')
assert value == "form-data"
assert parameters == {"name": "user"}
```

Use `MultipartBuilder` to produce a `multipart/form-data` body on the client side:

```python
from rust_multipart import MultipartBuilder

builder = MultipartBuilder(boundary=b"boundary")
builder.add_field("user", b"Potato")
builder.add_file("upload", "photo.png", b"\x89PNG...", content_type="image/png")
body = builder.build()

assert builder.content_type == "multipart/form-data; boundary=boundary"
assert body.startswith(b'--boundary\r\nContent-Disposition: form-data; name="user"')
```

Omit `boundary` and the builder generates a random 32-character one, available as `builder.boundary`. Double quotes
and line breaks in names and filenames are percent-escaped (`%22`, `%0D`, `%0A`), matching how browsers serialize
form submissions. `add_part()` takes raw headers when you need full control, and `build()` appends the closing
boundary and returns the complete body.

## Development

```bash
uv sync --group dev
uv run maturin develop
scripts/check
```

`scripts/check` runs formatting, linting, type checking, Python coverage, native Rust coverage, and the high-level test suite.
Both Python and Rust line coverage must remain at 100%.

Run the CodSpeed benchmarks locally with:

```bash
uv run pytest benchmarks --codspeed
```

## License

This project is licensed under the MIT License.
