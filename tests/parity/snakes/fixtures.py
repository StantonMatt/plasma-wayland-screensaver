#!/usr/bin/env python3
"""Load compressed/plain fixtures, or convert the recorder's QTest log into fixtures."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import gzip
import hashlib
import json
import lzma
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load_fixture(path):
    """Return one fixture dict. Food tuple columns are declared in README.md."""
    path = Path(path)
    opener = {".gz": gzip.open, ".xz": lzma.open}.get(path.suffix, open)
    with opener(path, "rt", encoding="utf-8") as source:
        fixture = json.load(source)
    if fixture.get("encoding") == "coordinate-delta2-v1":
        transform_coordinates(fixture, decode=True)
    return fixture


def transform_coordinates(fixture, decode=False):
    """Lossless second differences of quantized coordinates, keyed by identity."""
    history = {}

    def pair(key, values):
        previous, velocity = history.get(key, ([0, 0], [0, 0]))
        if decode:
            current_velocity = [velocity[i] + values[i] for i in range(2)]
            current = [previous[i] + current_velocity[i] for i in range(2)]
            output = [x / 1000000 for x in current]
        else:
            current = [round(x * 1000000) for x in values]
            current_velocity = [current[i] - previous[i] for i in range(2)]
            output = [current_velocity[i] - velocity[i] for i in range(2)]
        history[key] = (current, current_velocity)
        return output

    for frame in fixture["frames"]:
        for snake in frame["snakes"]:
            for index, segment in enumerate(snake["segments"]):
                segment[:] = pair(("snake", snake["index"], index), segment)
        for particle in frame["food"]:
            particle[1:3] = pair(("food", particle[0]), particle[1:3])
    if not decode:
        fixture["encoding"] = "coordinate-delta2-v1"
    else:
        fixture.pop("encoding", None)


def record(log, output):
    output.mkdir(parents=True, exist_ok=True)
    # Keep historical fixture keys, hashing the frozen oracle at those origins.
    sources = {
        name: hashlib.sha256((HERE / "oracle" / Path(name).name).read_bytes()).hexdigest()
        for name in ("qml/visuals/Snakes.qml", "qml/visuals/VisualUtils.js", "qml/visuals/FrameClock.qml")
    }
    fixture = None
    count = 0
    for line in log.read_text().splitlines():
        marker = "SNAKES_PARITY "
        if marker not in line:
            continue
        entry = json.loads(line.split(marker, 1)[1])
        kind, value = entry["type"], entry["value"]
        if kind == "begin":
            assert fixture is None, "unterminated fixture"
            fixture = value
            fixture["sourceSha256"] = sources
            fixture["frames"] = []
        elif kind == "frame":
            fixture["frames"].append(value)
        elif kind == "end":
            assert fixture["id"] == value["id"]
            assert sum(len(f["events"]) for f in fixture["frames"]) == value["totalEvents"]
            compressed = fixture["ticks"] >= 100
            original_frames = json.loads(json.dumps(fixture["frames"])) if compressed else None
            if compressed:
                transform_coordinates(fixture)
            data = (json.dumps(fixture, separators=(",", ":"), ensure_ascii=False) + "\n").encode()
            # Compress long traces, keeping focused cases directly readable.
            if compressed:
                transform_coordinates(fixture, decode=True)
                assert fixture["frames"] == original_frames, "coordinate encoding changed values"
            filename = fixture["id"] + (".json.xz" if compressed else ".json")
            (output / filename).write_bytes(lzma.compress(data, preset=9) if compressed else data)
            count += 1
            fixture = None
        else:
            raise ValueError(kind)
    assert fixture is None and count == 15, f"expected 15 complete fixtures, got {count}"
    total = sum(p.stat().st_size for p in output.iterdir())
    print("Fixture sizes: " + ", ".join(f"{p.name}={p.stat().st_size}" for p in sorted(output.iterdir())))
    assert total < 3 * 1024 * 1024, f"fixture budget exceeded: {total} bytes"
    print(f"Recorded {count} fixtures, {total:,} bytes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", nargs="?", type=Path)
    parser.add_argument("--record", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.record:
        if not args.output:
            parser.error("--record requires --output")
        record(args.record, args.output)
    elif args.fixture:
        print(json.dumps(load_fixture(args.fixture), indent=2))
    else:
        parser.error("pass a fixture, or --record LOG --output DIRECTORY")
