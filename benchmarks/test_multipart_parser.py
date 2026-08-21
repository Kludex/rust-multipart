from __future__ import annotations

from pytest_codspeed import BenchmarkFixture

from parser import File, MultipartParser

PAYLOAD = bytes(range(256)) * 4096
BODY = (
    b"--benchmark-boundary\r\n"
    b'Content-Disposition: form-data; name="upload"; filename="payload.bin"\r\n'
    b"Content-Type: application/octet-stream\r\n"
    b"\r\n" + PAYLOAD + b"\r\n--benchmark-boundary--\r\n"
)


def parse_whole_body() -> int:
    parser = MultipartParser(b"benchmark-boundary")
    parser.parse(BODY)
    part = parser.next_part()
    assert isinstance(part, File)
    return len(part.data)


def parse_streamed_body() -> int:
    parser = MultipartParser(b"benchmark-boundary")
    for offset in range(0, len(BODY), 16 * 1024):
        parser.parse(BODY[offset : offset + 16 * 1024])
    part = parser.next_part()
    assert isinstance(part, File)
    return len(part.data)


def test_parse_whole_body(benchmark: BenchmarkFixture) -> None:
    assert benchmark(parse_whole_body) == len(PAYLOAD)


def test_parse_streamed_body(benchmark: BenchmarkFixture) -> None:
    assert benchmark(parse_streamed_body) == len(PAYLOAD)
