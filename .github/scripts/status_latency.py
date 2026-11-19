"""Measure full HTTP status responses after migrating a five-million-row v2 archive."""

import json
from pathlib import Path
import re
import signal
import sqlite3
import statistics
import subprocess
import sys
import tempfile
import time
import urllib.request

binary = str(Path(sys.argv[1]).resolve())
schema = Path(__file__).resolve().parents[2] / "src/schema.sql"
with tempfile.TemporaryDirectory() as tmp:
    data = Path(tmp)
    log = data / "fixture.log"
    log.touch()
    cli = [binary, "--data-dir", str(data)]
    subprocess.run(
        [*cli, "source", "add-file", "--name", "fixture", "--path", str(log)],
        check=True,
    )
    subprocess.run([*cli, "source", "pause", "fixture"], check=True)
    subprocess.run([*cli, "config", "poll", "600"], check=True)
    (data / "archives").mkdir(exist_ok=True)
    with sqlite3.connect(data / "archives/fixture.db") as db:
        db.executescript(schema.read_text())
        db.execute(
            """WITH RECURSIVE seq(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM seq WHERE i<5000000)
                      INSERT INTO entries SELECT i,CAST(i AS TEXT),NULL,0,0,:now,:now,'event','fixture' FROM seq""",
            {"now": time.time_ns()},
        )
    service = subprocess.Popen([*cli, "web"], stdout=subprocess.PIPE, text=True)
    try:
        for line in service.stdout:
            match = re.search(r"http://127\.0\.0\.1:\d+", line)
            if match:
                origin = match.group()
                break
        page = urllib.request.urlopen(origin).read().decode()
        token = re.search(r'name="loglem-token" content="([^"]+)', page)[1]
        request = urllib.request.Request(
            origin + "/api/status",
            b"{}",
            headers={
                "Content-Type": "application/json",
                "X-Loglem-Token": token,
                "Origin": origin,
            },
        )
        for _ in range(300):
            with urllib.request.urlopen(request) as response:
                result = json.load(response)
            if result["collection"]["generation"]:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("Collector did not finish")
        samples = []
        for _ in range(50):
            start = time.perf_counter()
            with urllib.request.urlopen(request, timeout=10) as response:
                result = json.load(response)
            assert result["sources"][0]["entries"] == 5_000_000
            samples.append(1000 * (time.perf_counter() - start))
        median = statistics.median(samples)
        print(
            f"5,000,000 rows; 50 full status responses; median {median:.3f} ms, p95 {sorted(samples)[47]:.3f} ms",
            flush=True,
        )
        assert sorted(samples)[47] < 10
    finally:
        service.send_signal(signal.SIGTERM)
        service.wait(timeout=40)
