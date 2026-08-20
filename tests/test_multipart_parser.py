from __future__ import annotations

from collections.abc import Iterable

import pytest

from parser import Field, File, MultipartParser, MultipartPart, MultipartState


def feed(parser: MultipartParser, data: bytes, sizes: Iterable[int]) -> None:
    offset = 0
    for size in sizes:
        parser.parse(data[offset : offset + size])
        offset += size
    if offset < len(data):
        parser.parse(data[offset:])


def drain_events(parser: MultipartParser) -> list[MultipartPart.Header | MultipartPart.Body]:
    events: list[MultipartPart.Header | MultipartPart.Body] = []
    while (event := parser.next_event()) is not None:
        events.append(event)
    return events


def test_constructor_validation() -> None:
    with pytest.raises(ValueError, match="Boundary length must be between 1 and 70 characters"):
        MultipartParser(b"")
    MultipartParser(b"x" * 70)
    with pytest.raises(ValueError, match="Boundary length must be between 1 and 70 characters"):
        MultipartParser(b"x" * 71)
    with pytest.raises(RuntimeError, match="The only supported charset is 'utf8'"):
        MultipartParser(b"boundary", header_charset="ascii")


@pytest.fixture
def parser() -> MultipartParser:
    return MultipartParser(b"boundary")


def test_parser_state_transitions(parser: MultipartParser) -> None:
    assert parser.state == MultipartState.PREAMBLE

    parser.parse(b"--bound")
    assert parser.state == MultipartState.PREAMBLE

    parser.parse(b"ary\r\n")
    assert parser.state == MultipartState.HEADER

    parser.parse(b"Content-Disposition: form-data; name=field\r\n")
    assert parser.state == MultipartState.HEADER

    parser.parse(b"\r\n")
    assert parser.state == MultipartState.BODY

    parser.parse(b"value\r\n--bound")
    assert parser.state == MultipartState.BODY

    parser.parse(b"ary--")
    assert parser.state == MultipartState.END


def test_parser_queues_start_empty(parser: MultipartParser) -> None:
    assert parser.next_event() is None
    assert parser.next_part() is None


def test_streams_fields_and_binary_files() -> None:
    body = (
        b"ignored preamble\r\n"
        b"--boundary\r\n"
        b"Content-Disposition: form-data; name=field\r\n"
        b"\r\n"
        b"value\r\n"
        b"--boundary\r\n"
        b'Content-Disposition: FORM-DATA; name="upload"; filename="a\\"b.bin"\r\n'
        b"Content-Type: application/octet-stream; charset=binary\r\n"
        b"\r\n"
        b"\x00\xff\r\n\x80\r\n"
        b"--boundary--\r\n"
        b"ignored epilogue"
    )
    parser = MultipartParser(b"boundary")

    feed(parser, body, [1] * len(body))

    assert parser.state == MultipartState.END
    field = parser.next_part()
    assert isinstance(field, Field)
    assert field.name == "field"
    assert field.content_type == "text/plain"
    assert field.charset == "utf-8"
    assert field.data == b"value"

    file = parser.next_part()
    assert isinstance(file, File)
    assert file.name == "upload"
    assert file.filename == 'a"b.bin'
    assert file.content_type == "application/octet-stream"
    assert file.charset == "binary"
    assert file.data == b"\x00\xff\r\n\x80"
    assert parser.next_part() is None

    events = drain_events(parser)
    headers = [event for event in events if isinstance(event, MultipartPart.Header)]
    bodies = [event for event in events if isinstance(event, MultipartPart.Body)]
    assert [(event.name, event.value) for event in headers] == [
        ("content-disposition", "form-data; name=field"),
        ("content-disposition", 'FORM-DATA; name="upload"; filename="a\\"b.bin"'),
        ("content-type", "application/octet-stream; charset=binary"),
    ]
    assert b"".join(event.data for event in bodies) == b"value\x00\xff\r\n\x80"
    assert sum(event.complete for event in bodies) == 2
    assert repr(headers[0]) == 'Header(name="content-disposition", value="form-data; name=field")'
    assert repr(bodies[-1]).startswith("Body(data=")

    parser.parse(b"more epilogue")
    assert parser.state == MultipartState.END


def test_preserves_terminal_crlf_in_body() -> None:
    parser = MultipartParser(b"boundary")
    parser.parse(b"--boundary\r\nContent-Disposition: form-data; name=field\r\n\r\nvalue\r\n\r\n--boundary--")

    part = parser.next_part()
    assert isinstance(part, Field)
    assert part.data == b"value\r\n"


def test_accepts_transport_padding_and_closing_without_crlf() -> None:
    body = b"--boundary \t\r\nContent-Disposition: form-data; name=field\r\n\r\nvalue\r\n--boundary--"
    parser = MultipartParser(b"boundary")

    feed(parser, body, [9, 1, 2, 3])

    assert parser.state == MultipartState.END
    part = parser.next_part()
    assert isinstance(part, Field)
    assert part.data == b"value"


def test_treats_near_boundaries_as_body_data() -> None:
    body = (
        b"--boundary\r\n"
        b"Content-Disposition: form-data; name=field\r\n"
        b"\r\n"
        b"alpha\r\n--boundaryX\r\n--boundary-!\r\nomega\r\n"
        b"--boundary--\r\n"
    )
    parser = MultipartParser(b"boundary")

    feed(parser, body, [1] * len(body))

    part = parser.next_part()
    assert isinstance(part, Field)
    assert part.data == b"alpha\r\n--boundaryX\r\n--boundary-!\r\nomega"


def test_ignores_false_preamble_candidates() -> None:
    parser = MultipartParser(b"boundary")
    body = (
        b"prefix--boundaryX\r\n"
        b"--boundaryZ\r\n"
        b"--boundary\r\n"
        b"Content-Disposition: form-data; name=field\r\n"
        b"\r\n"
        b"value\r\n"
        b"--boundary--"
    )

    feed(parser, body, [4, 2, 1, 8])

    assert parser.state == MultipartState.END


def test_accepts_empty_multipart_body() -> None:
    parser = MultipartParser(b"boundary")

    parser.parse(b"--boundary--")

    assert parser.state == MultipartState.END
    assert parser.next_part() is None


def test_reports_incomplete_boundaries_by_state() -> None:
    parser = MultipartParser(b"boundary")
    parser.parse(b"--bound")
    assert parser.state == MultipartState.PREAMBLE

    parser = MultipartParser(b"boundary")
    parser.parse(b"--boundary\r\nContent-Disposition: form-data; name=field\r\n\r\nvalue\r\n--boundary-")
    assert parser.state == MultipartState.BODY
    assert parser.next_part() is None


def test_rejects_bare_line_feeds() -> None:
    parser = MultipartParser(b"boundary")
    with pytest.raises(ValueError, match="Invalid line break after delimiter"):
        parser.parse(b"--boundary\n")

    parser = MultipartParser(b"boundary")
    parser.parse(b"--boundary\r\n")
    with pytest.raises(ValueError, match="Invalid line break in header"):
        parser.parse(b"Content-Disposition: form-data; name=field\n")

    parser = MultipartParser(b"boundary")
    parser.parse(b"--boundary\r\nContent-Disposition: form-data; name=field\r\n\r\nvalue\r\n")
    with pytest.raises(ValueError, match="Invalid line break after delimiter"):
        parser.parse(b"--boundary\n")


def test_rejects_malformed_headers() -> None:
    malformed = [
        (b"Header without colon\r\n", "Malformed header"),
        (b"\xff: value\r\n", "Invalid header name"),
        (b"Name: \xff\r\n", "Invalid header value"),
        (b": value\r\n", "Missing header name"),
    ]

    for header, message in malformed:
        parser = MultipartParser(b"boundary")
        parser.parse(b"--boundary\r\n")
        with pytest.raises(ValueError, match=message):
            parser.parse(header)


@pytest.mark.parametrize(
    ("header", "message"),
    [
        (b"Content-Type: ; charset=utf-8\r\n", "Missing header name"),
        (b"Content-Type: text/plain; charset\r\n", "Missing parameter value"),
        (b"Content-Type: text/plain; =utf-8\r\n", "Missing parameter key"),
        (b'Content-Type: text/plain; charset="utf-8"junk\r\n', "Malformed quoted parameter"),
        (b'Content-Type: text/plain; charset="utf-8\\\r\n', "Malformed quoted parameter"),
    ],
)
def test_rejects_malformed_header_parameters(header: bytes, message: str) -> None:
    parser = MultipartParser(b"boundary")
    with pytest.raises(ValueError, match=message):
        parser.parse(b"--boundary\r\nContent-Disposition: form-data; name=field\r\n" + header + b"\r\n")


@pytest.mark.parametrize(
    ("disposition", "message"),
    [
        (None, "Missing content-disposition header"),
        (b"attachment; name=field", "Invalid content-disposition"),
        (b"form-data", "Parameter 'name' not found in content-disposition"),
    ],
)
def test_validates_content_disposition(disposition: bytes | None, message: str) -> None:
    header = b"" if disposition is None else b"Content-Disposition: " + disposition + b"\r\n"
    parser = MultipartParser(b"boundary")

    with pytest.raises(ValueError, match=message):
        parser.parse(b"--boundary\r\n" + header + b"\r\n")


def test_last_duplicate_header_wins() -> None:
    parser = MultipartParser(b"boundary")
    parser.parse(
        b"--boundary\r\n"
        b"Content-Disposition: form-data; name=first\r\n"
        b"Content-Disposition: form-data; name=second\r\n"
        b"\r\n"
        b"value\r\n"
        b"--boundary--"
    )

    part = parser.next_part()
    assert isinstance(part, Field)
    assert part.name == "second"


def test_enforces_maximum_size() -> None:
    parser = MultipartParser(b"boundary", max_size=3)
    parser.parse(b"abc")

    with pytest.raises(RuntimeError, match="Data exceeds maximum size"):
        parser.parse(b"d")
