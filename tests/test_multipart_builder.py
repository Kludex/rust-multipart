from __future__ import annotations

import pytest

from rust_multipart import MultipartBuilder, MultipartParser, PartBegin, PartData


def parse(body: bytes, boundary: bytes) -> list[tuple[list[tuple[bytes, bytes]], bytes]]:
    parser = MultipartParser(boundary)
    parts: list[tuple[list[tuple[bytes, bytes]], bytes]] = []
    for event in parser.feed(body):
        if isinstance(event, PartBegin):
            parts.append((event.headers, b""))
        elif isinstance(event, PartData):
            headers, data = parts[-1]
            parts[-1] = (headers, data + event.data)
    parser.finish()
    return parts


def test_round_trip() -> None:
    builder = MultipartBuilder(boundary=b"boundary")
    builder.add_field("name", b"value")
    builder.add_file("upload", "photo.png", b"\x89PNG...", content_type="image/png")
    builder.add_part([(b"X-Custom", b"yes")], b"raw part")
    body = builder.build()

    assert parse(body, b"boundary") == [
        ([(b"Content-Disposition", b'form-data; name="name"')], b"value"),
        (
            [
                (b"Content-Disposition", b'form-data; name="upload"; filename="photo.png"'),
                (b"Content-Type", b"image/png"),
            ],
            b"\x89PNG...",
        ),
        ([(b"X-Custom", b"yes")], b"raw part"),
    ]


def test_generated_boundary_round_trips() -> None:
    builder = MultipartBuilder()
    other = MultipartBuilder()
    assert builder.boundary != other.boundary
    assert len(builder.boundary) == 32
    builder.add_field("a", b"1")
    assert parse(builder.build(), builder.boundary) == [([(b"Content-Disposition", b'form-data; name="a"')], b"1")]


def test_content_type() -> None:
    assert MultipartBuilder(boundary=b"simple").content_type == "multipart/form-data; boundary=simple"
    assert MultipartBuilder(boundary=b"has space").content_type == 'multipart/form-data; boundary="has space"'


def test_escapes_name_and_filename() -> None:
    builder = MultipartBuilder(boundary=b"boundary")
    builder.add_file('a"b\r\n', 'file"\r\n.txt', b"data")
    [(headers, _)] = parse(builder.build(), b"boundary")
    assert headers == [(b"Content-Disposition", b'form-data; name="a%22b%0D%0A"; filename="file%22%0D%0A.txt"')]


def test_empty_body() -> None:
    assert MultipartBuilder(boundary=b"boundary").build() == b"--boundary--\r\n"


def test_rejects_invalid_boundaries() -> None:
    for boundary in [b"", b"a" * 71, b"bad\x00", b"no\r\nbreaks", b"trailing space "]:
        with pytest.raises(ValueError):
            MultipartBuilder(boundary=boundary)


def test_rejects_invalid_headers() -> None:
    builder = MultipartBuilder(boundary=b"boundary")
    invalid = [
        ([(b"", b"value")], "Missing header name"),
        ([(b"Bad:Name", b"value")], "Invalid character in header name"),
        ([(b"Bad\r\nName", b"value")], "Invalid character in header name"),
        ([(b"Name", b"bad\r\nvalue")], "Invalid line break in header value"),
    ]
    for headers, message in invalid:
        with pytest.raises(ValueError, match=message):
            builder.add_part(headers, b"data")


def test_rejects_use_after_build() -> None:
    builder = MultipartBuilder(boundary=b"boundary")
    builder.build()
    with pytest.raises(RuntimeError, match="finished"):
        builder.add_field("a", b"1")
    with pytest.raises(RuntimeError, match="finished"):
        builder.build()
