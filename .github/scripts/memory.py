"""Linux release benchmark. All logs and configuration are temporary."""

import json
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    data = root / "data"
    log = root / "access.log"
    cli = [binary, "--data-dir", str(data)]
    now = int(time.time())
    with log.open("w") as out:
        for i in range(200_000):
            row = {
                "ts": now - (199999 - i) * 12,
                "request": {
                    "uri": f"/p/{i % 512}?v={i}",
                    "client_ip": f"192.0.{i//256%256}.{i%256}",
                },
                "status": 200,
                "duration": 0.015,
                "agent": "synthetic " * 41,
            }
            out.write(json.dumps(row, separators=(",", ":")) + "\n")

    def run(*args):
        return subprocess.check_output([*cli, *args], text=True)

    run("source", "add-file", "--name", "web", "--path", str(log), "--days", "31")
    run("config", "poll", "600")
    run("collect")
    service = subprocess.Popen([*cli, "web"], stdout=subprocess.PIPE, text=True)
    try:
        for line in service.stdout:
            match = re.search(r"http://127\.0\.0\.1:\d+", line)
            if match:
                address = match.group()
                break
        page = urllib.request.urlopen(address).read().decode()
        token = re.search(r'name="loglem-token" content="([^"]+)', page)[1]

        def api(route, body):
            req = urllib.request.Request(
                address + "/api/" + route,
                json.dumps(body).encode(),
                {
                    "Content-Type": "application/json",
                    "Origin": address,
                    "X-Loglem-Token": token,
                },
            )
            with urllib.request.urlopen(req, timeout=90) as reply:
                return json.load(reply)

        for _ in range(600):
            if api("status", {})["collection"]["generation"]:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("Collector did not finish")

        def timed(route, args):
            start = time.perf_counter()
            result = api(route, args)
            return result, time.perf_counter() - start

        for _ in range(10):
            try:
                api("stats", {"source": "absent"})
            except urllib.error.HTTPError as error:
                assert error.code == 400
            else:
                raise AssertionError("Expected an error")

        def memory():
            status = Path(f"/proc/{service.pid}/status").read_text()
            rollup = Path(f"/proc/{service.pid}/smaps_rollup").read_text()
            return {
                k: int(
                    re.search("^" + k + r":\s+(\d+)", status + "\n" + rollup, re.M)[1]
                )
                for k in ["VmRSS", "VmHWM", "RssAnon", "Pss"]
            }

        after_errors = memory()
        rows, search = timed("search", {"source": "web", "text": "impossible-token"})
        assert rows["rows"] == []
        found, _ = timed("search", {"source": "web", "text": "/p/1"})
        assert found["rows"]
        stats, elapsed = timed("stats", {"source": "web", "days": 30})
        assert stats["requests"] == 200000
        time.sleep(1)
        final = memory()
        size = sum(p.stat().st_size for p in (data / "archives").glob("web.db*"))
        print(
            json.dumps(
                dict(
                    final_kib=final,
                    errors_kib=after_errors,
                    stats_s=elapsed,
                    search_s=search,
                    archive_bytes=size,
                    input_bytes=log.stat().st_size,
                )
            ),
            flush=True,
        )
        assert after_errors["RssAnon"] < 2048
        assert final["VmHWM"] < 12 * 1024 and final["RssAnon"] < 3072
        assert final["VmRSS"] < 9 * 1024 and elapsed < 0.3
        assert search < 1.0 and size < 20_000_000
    finally:
        service.send_signal(signal.SIGTERM)
        service.wait(timeout=40)
