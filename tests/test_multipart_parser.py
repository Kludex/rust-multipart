from __future__ import annotations

from collections.abc import Iterable

import pytest

from rust_multipart import MultipartParser, MultipartState, PartBegin, PartData, PartEnd

Event = PartBegin | PartData | PartEnd


def feed(parser: MultipartParser, data: bytes, sizes: Iterable[int]) -> list[Event]:
    events: list[Event] = []
    offset = 0
    for size in sizes:
        events.extend(parser.feed(data[offset : offset + size]))
        offset += size
    if offset < len(data):
        events.extend(parser.feed(data[offset:]))
    return events


def collect_parts(events: list[Event]) -> list[tuple[list[tuple[bytes, bytes]], bytes]]:
    parts: list[tuple[list[tuple[bytes, bytes]], bytes]] = []
    headers: list[tuple[bytes, bytes]] = []
    data = b""
    for event in events:
        if isinstance(event, PartBegin):
            headers, data = event.headers, b""
        elif isinstance(event, PartData):
            data += event.data
        else:
            parts.append((headers, data))
    return parts


def test_constructor_validation() -> None:
    with pytest.raises(ValueError, match="Boundary length must be between 1 and 70 characters"):
        MultipartParser(b"")
    MultipartParser(b"x" * 70)
    with pytest.raises(ValueError, match="Boundary length must be between 1 and 70 characters"):
        MultipartParser(b"x" * 71)


@pytest.fixture
def parser() -> MultipartParser:
    return MultipartParser(b"boundary")


def test_parser_preamble(parser: MultipartParser) -> None:
    parser.feed(b"--boundary\r")

    assert parser.state == MultipartState.PREAMBLE


def test_parser_header(parser: MultipartParser) -> None:
    parser.feed(b"--boundary\r\n")

    assert parser.state == MultipartState.HEADER


def test_parser_body(parser: MultipartParser) -> None:
    events = parser.feed(b"--boundary\r\nContent-Disposition: form-data; name=field\r\n\r\n")

    assert parser.state == MultipartState.BODY
    assert len(events) == 1
    begin = events[0]
    assert isinstance(begin, PartBegin)
    assert begin.headers == [(b"Content-Disposition", b"form-data; name=field")]


def test_parser_end(parser: MultipartParser) -> None:
    parser.feed(b"--boundary--")

    assert parser.state == MultipartState.END
    parser.finish()


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

    events = feed(parser, body, [1] * len(body))

    assert parser.state == MultipartState.END
    parser.finish()
    parts = collect_parts(events)
    assert parts == [
        ([(b"Content-Disposition", b"form-data; name=field")], b"value"),
        (
            [
                (b"Content-Disposition", b'FORM-DATA; name="upload"; filename="a\\"b.bin"'),
                (b"Content-Type", b"application/octet-stream; charset=binary"),
            ],
            b"\x00\xff\r\n\x80",
        ),
    ]

    assert parser.feed(b"more epilogue") == []
    assert parser.state == MultipartState.END


def test_event_reprs(parser: MultipartParser) -> None:
    events = parser.feed(b"--boundary\r\nContent-Disposition: form-data; name=x\r\n\r\ndata\r\n--boundary--")

    begin, data, end = events
    assert repr(begin).startswith("PartBegin(headers=[")
    assert repr(data) == "PartData(data=b'data')"
    assert repr(end) == "PartEnd()"


def test_preserves_terminal_crlf_in_body(parser: MultipartParser) -> None:
    events = parser.feed(b"--boundary\r\nContent-Disposition: form-data; name=field\r\n\r\nvalue\r\n\r\n--boundary--")

    assert collect_parts(events)[0][1] == b"value\r\n"


def test_accepts_transport_padding_and_closing_without_crlf() -> None:
    body = b"--boundary \t\r\nContent-Disposition: form-data; name=field\r\n\r\nvalue\r\n--boundary--"
    parser = MultipartParser(b"boundary")

    events = feed(parser, body, [9, 1, 2, 3])

    assert parser.state == MultipartState.END
    assert collect_parts(events)[0][1] == b"value"


def test_treats_near_boundaries_as_body_data() -> None:
    body = (
        b"--boundary\r\n"
        b"Content-Disposition: form-data; name=field\r\n"
        b"\r\n"
        b"alpha\r\n--boundaryX\r\n--boundary-!\r\nomega\r\n"
        b"--boundary--\r\n"
    )
    parser = MultipartParser(b"boundary")

    events = feed(parser, body, [1] * len(body))

    assert collect_parts(events)[0][1] == b"alpha\r\n--boundaryX\r\n--boundary-!\r\nomega"


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


def test_accepts_empty_multipart_body(parser: MultipartParser) -> None:
    events = parser.feed(b"--boundary--")

    assert parser.state == MultipartState.END
    assert events == []
    parser.finish()


def test_finish_rejects_truncated_message(parser: MultipartParser) -> None:
    parser.feed(b"--boundary\r\nContent-Disposition: form-data; name=field\r\n\r\nvalue")

    with pytest.raises(ValueError, match="closing boundary not received"):
        parser.finish()


def test_reports_incomplete_boundaries_by_state() -> None:
    parser = MultipartParser(b"boundary")
    parser.feed(b"--bound")
    assert parser.state == MultipartState.PREAMBLE

    parser = MultipartParser(b"boundary")
    events = parser.feed(b"--boundary\r\nContent-Disposition: form-data; name=field\r\n\r\nvalue\r\n--boundary-")
    assert parser.state == MultipartState.BODY
    assert not any(isinstance(event, PartEnd) for event in events)


def test_rejects_bare_line_feeds() -> None:
    parser = MultipartParser(b"boundary")
    with pytest.raises(ValueError, match="Invalid line break after delimiter"):
        parser.feed(b"--boundary\n")

    parser = MultipartParser(b"boundary")
    parser.feed(b"--boundary\r\n")
    with pytest.raises(ValueError, match="Invalid line break in header"):
        parser.feed(b"Content-Disposition: form-data; name=field\n")

    parser = MultipartParser(b"boundary")
    parser.feed(b"--boundary\r\nContent-Disposition: form-data; name=field\r\n\r\nvalue\r\n")
    with pytest.raises(ValueError, match="Invalid line break after delimiter"):
        parser.feed(b"--boundary\n")


def test_rejects_malformed_headers() -> None:
    malformed = [
        (b"Header without colon\r\n", "Malformed header"),
        (b": value\r\n", "Missing header name"),
    ]

    for header, message in malformed:
        parser = MultipartParser(b"boundary")
        parser.feed(b"--boundary\r\n")
        with pytest.raises(ValueError, match=message):
            parser.feed(header)


def test_preserves_raw_header_bytes_and_order() -> None:
    parser = MultipartParser(b"boundary")
    events = parser.feed(
        b"--boundary\r\n"
        b"X-First: one\r\n"
        b"Content-Disposition: form-data; name=first\r\n"
        b"X-First: two\r\n"
        b"\r\n"
        b"value\r\n"
        b"--boundary--"
    )

    begin = events[0]
    assert isinstance(begin, PartBegin)
    assert begin.headers == [
        (b"X-First", b"one"),
        (b"Content-Disposition", b"form-data; name=first"),
        (b"X-First", b"two"),
    ]


def test_enforces_maximum_size() -> None:
    parser = MultipartParser(b"boundary", max_size=3)
    parser.feed(b"abc")

    with pytest.raises(RuntimeError, match="Data exceeds maximum size"):
        parser.feed(b"d")
