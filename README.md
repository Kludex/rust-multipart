# Rust Parser for `multipart/form-data`

> [!WARNING]
> This project is a work in progress and is not ready for production use.

This package provides a Sans-IO parser for [RFC 7578](https://datatracker.ietf.org/doc/html/rfc7578) `multipart/form-data`.
It is heavily inspired by [defnull/multipart](https://github.com/defnull/multipart).

## Installation

```bash
pip install multipart-parser
```

## Usage

```python
from parser import Field, MultipartParser

parser = MultipartParser(boundary=b"boundary")
parser.parse(b'--boundary\r\nContent-Disposition: form-data; name="user"\r\n\r\nPotato\r\n--boundary--\r\n')

field = parser.next_part()
assert isinstance(field, Field)
assert field.name == "user"
assert field.data == b"Potato"
```

You can call `parse()` repeatedly with partial input. The parser preserves binary part data and emits completed parts through
`next_part()`.

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
