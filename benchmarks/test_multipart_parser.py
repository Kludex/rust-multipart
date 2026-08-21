from __future__ import annotations

from pytest_codspeed import BenchmarkFixture

from parser import MultipartParser, PartData

BOUNDARY = b"benchmark-boundary"
CHUNK_SIZE = 64 * 1024


def build_file_upload(payload: bytes) -> bytes:
    return (
        b"--" + BOUNDARY + b"\r\n"
        b'Content-Disposition: form-data; name="upload"; filename="payload.bin"\r\n'
        b"Content-Type: application/octet-stream\r\n"
        b"\r\n" + payload + b"\r\n--" + BOUNDARY + b"--\r\n"
    )


def build_form_fields(count: int) -> bytes:
    part = b"--" + BOUNDARY + b'\r\nContent-Disposition: form-data; name="field"\r\n\r\nvalue\r\n'
    return part * count + b"--" + BOUNDARY + b"--\r\n"


LARGE_PAYLOAD = bytes(range(256)) * 4096
LARGE_UPLOAD = build_file_upload(LARGE_PAYLOAD)
# Worst case for boundary search: the payload is full of near-boundary prefixes.
WORST_CASE_PAYLOAD = (b"\r\n--" + BOUNDARY[:-1] + b"!") * (1024 * 1024 // (len(BOUNDARY) + 3))
WORST_CASE_UPLOAD = build_file_upload(WORST_CASE_PAYLOAD)
SMALL_FIELDS = build_form_fields(1000)


def parse(body: bytes) -> int:
    parser = MultipartParser(BOUNDARY)
    total = 0
    for offset in range(0, len(body), CHUNK_SIZE):
        for event in parser.feed(body[offset : offset + CHUNK_SIZE]):
            if isinstance(event, PartData):
                total += len(event.data)
    parser.finish()
    return total


def test_parse_large_upload(benchmark: BenchmarkFixture) -> None:
    assert benchmark(parse, LARGE_UPLOAD) == len(LARGE_PAYLOAD)


def test_parse_worst_case_upload(benchmark: BenchmarkFixture) -> None:
    assert benchmark(parse, WORST_CASE_UPLOAD) == len(WORST_CASE_PAYLOAD)


def test_parse_small_fields(benchmark: BenchmarkFixture) -> None:
    assert benchmark(parse, SMALL_FIELDS) == 5 * 1000
