# TODO

- [ ] `MultipartBuilder` to build `multipart/form-data` request bodies.
- [ ] Starlette adapter proving out the event API end to end.

## Done

- [X] Non-buffering event API (`feed()` / `finish()`, `PartBegin` / `PartData` / `PartEnd`).
- [X] `parse_options_header()` for header values and their parameters.
- [X] Message size limit (`max_size`).
- [X] Header limits (`max_header_count`, `max_header_size`), matching python-multipart defaults.

## Out of scope

Part assembly, `Content-Disposition` validation, `_charset_` handling, per-file size limits, and
spilling to temporary files belong to the consumer (e.g. a Starlette adapter), not to the sans-io parser.
