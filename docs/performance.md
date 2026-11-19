# Memory, disk and response times

[English](performance.md) | [Italiano](it/performance.md)

Loglem keeps each source in a local SQLite archive. Collection prepares statistics and alert state once per cycle, so several browser tabs read the same results.

## Release measurements

Test on Linux ARM64 with glibc, two virtual CPUs and one source. The generator writes 200,000 synthetic HTTP access lines over about 28 days, with more than 65,000 IP addresses and 512 paths. The input file is 106.27 MB. Times include the HTTP response. Disk space includes the database and the SQLite side files present during the measurement.

| Measurement | Result |
|---|---:|
| Archive on disk | 9.32 MB |
| Search over the whole history, no match | 239 ms |
| Statistics for the last 30 days | 1.47 ms |
| RSS one second after the statistics | 4.57 MiB |
| PSS at the same moment | 3.60 MiB |
| Peak RSS of the web process during the test | 5.35 MiB |
| Anonymous memory after 10 HTTP 400 responses | 440 KiB |
| Full status over 5 million lines, 50 requests | 0.83 ms median, 1.23 ms at the 95th percentile |

RSS counts every resident page of the process. PSS shares the cost of shared pages among the processes that use them. Both refer to `loglem web`, started after the first import. The compression ratio depends on how repetitive the log text is.

## What was done

- [x] **Diagnostics.** HTTP errors, collection errors and panics are still recorded. Backtraces follow `RUST_BACKTRACE` and `RUST_LIB_BACKTRACE`. The collector can resume after a panic.
- [x] **Process memory.** On Linux with glibc, Loglem limits malloc arenas to two and sets the release threshold to 128 KiB. After an HTTP request and at the end of each collection cycle, it asks the allocator to give free pages back.
- [x] **Statistics.** The collector updates daily totals while collecting. Windows that start in the middle of a day only count lines after the limit. Request counts, HTTP codes and daily totals are exact. Rankings keep up to 256 addresses and 256 paths per summary, with paths cut at 256 bytes. When the counters are full, the page and the terminal say «Approximate ranking». Erasing, masking and expiry refresh the affected summaries.
- [x] **Status and alerts.** Line count and oldest date are kept up to date with each archive change. The collector checks alert rules on new lines only. The page reads the saved status; while polling it sends the last generation it received, and gets only the collection state if nothing changed.
- [x] **Retention of the originals.** The collector keeps the description of how long the originals live for ten minutes. A configuration change clears it.
- [x] **Shared journal.** Active journal sources share one read per cycle. Each source applies its own filters, expiry and cursor.
- [x] **Archive in blocks.** Schema 4 upgrades older archives. Lines are packed into gzip blocks per file every 1,000 lines or after five minutes. Search reads blocks as a stream, newest first. Expiry drops whole blocks when it can; partial changes rewrite only the affected lines. Seals use the original text, decompressed in arrival order. Tests cover a seal made in the older format, re-reading the same source, partial expiry, erasing, masking, backup and restore.
- [x] **Compact deduplication keys.** For journal, Podman and Kubernetes, each line's key is stored as the first 16 bytes of its SHA-256 digest, without the index that copied it a second time. Schema 4 archives are brought to the new format (schema 5) on first open.
- [x] **Executable.** The release uses its own command-line parser and the system `curl` for updates, timestamps and `tail --server`. These three features need `curl` on the command path; the container image includes it. Updates are still checked with the Ed25519 signature. SQLite is built without FTS3, FTS5, R-Tree, extension loading and deprecated APIs, and keeps `dbstat`.
- [x] **Automatic checks.** Linux CI on x86-64 and ARM64 runs the two benchmarks below. It also checks that answers are correct, memory after errors stays low, and archive, search, statistics and status stay under their limits.

### Release settings

On the same 106.27 MB load, moving the Rust profile from `s` to `z` with glibc shrank the ARM64 binary from 3.95 to 3.62 MB, and full search went from 188 to 222 ms. The release uses `z`. Compared with profile `s`, mimalloc with `MIMALLOC_PURGE_DELAY=0` reached 13.80 MiB peak RSS and 6.96 MiB anonymous memory after errors. The release uses the system allocator.

## Reproduce the measurements

On Linux with Python 3 and a Rust toolchain that builds the project, from the repository root:

```sh
cargo build --release --locked
python3 .github/scripts/memory.py target/release/loglem
python3 .github/scripts/status_latency.py target/release/loglem
```

The scripts create temporary folders and query the service on loopback. The second one builds a 5-million-line history, waits for its migration, and measures 50 full status responses. Creation and the first migration are not counted.

## Tests on a real Linux system

Run on 19 November 2026 on commit `a528000`, on a real Linux computer rather than in a container. They cover what had so far only been tested on macOS or in a container: journald, Podman, logrotate, Caddy and the systemd service.

| Component | Version |
|---|---|
| System | Linux Mint 22.3 (Ubuntu 24.04 base), kernel 7.0, x86_64, 16 CPUs |
| Libraries | glibc 2.39, systemd 255 |
| Programs | rootless Podman 4.9.3 (log driver: journald), logrotate 3.21.0, Caddy 2.11.9 in a container |
| Machine journal | 586 MB in 100 files, about 566,000 entries, 85 days |
| Loglem | release built with `cargo build --release --locked`, 4.76 MB executable |

Every test used its own data folder and containers created for the purpose. Line counts were checked against the source: `journalctl` for the journal, and the sequence number written into every line for files and containers.

### Automated tests and CI benchmarks

| Test | Result |
|---|---|
| `cargo test --release` | 93 tests passed |
| `memory.py` (106.27 MB of synthetic access logs) | 8.92 MB archive, 105 ms search with no matches, 2.0 ms statistics, final RSS 6.0 MiB, PSS 4.2 MiB, peak 6.8 MiB, 652 KiB anonymous memory after 10 errors |
| `status_latency.py` (5 million rows) | 1.21 ms median, 1.43 ms 95th percentile over 50 responses |

### Behaviour

| Test | Result |
|---|---|
| **journald**: three sources filtered by program and priority, shared read | ✅ 1,400, 1,100 and 100 lines, same as `journalctl` |
| **journald**: `kill -9` while 6,000 lines arrive, then restart | ✅ 9,100 of 9,100 lines, no duplicates. Loglem reports the unclean stop on startup |
| **journald**: import of the whole journal (365 days) | ✅ 566,525 of 566,527 entries (the 2 missing arrived after the read), in 91 s |
| **journald**: `--unit` source for a systemd user service (also in the shared read) | ✅ 201 lines for 200 written plus the service start, 50 of 50 in the shared read |
| **Podman**: journald-driver container, read by ID and by name through the journal | ✅ 500 of 500 lines in both cases |
| **Podman**: `k8s-file` driver container with `max-size` | ✅ 500 of 500 lines |
| **Podman**: container recreated with the same name | ✅ The by-name source continues (800 of 800). The by-ID source reports a gap with a clear message, and reconnecting recovers the 300 lines |
| **Podman**: container removed while Loglem is stopped | ✅ As documented, 200 lines lost and the gap shown on the source |
| **logrotate**: `create`, with and without `compress`, with `delaycompress`, with `dateext` (4 configurations, repeated rotations including some with no read in between) | ✅ 2,150 of 2,150 lines, no duplicates and none missing |
| **logrotate**: `copytruncate`, with and without compression, with `delaycompress` and `dateext` (4 configurations), also with the copy read before the active file | ✅ 2,150 of 2,150 lines, no duplicates and none missing |
| Real **Caddy** (`roll_size 1MiB`, `roll_keep 3`), 8,000 requests in 33 s with `loglem serve` running | ✅ 8,000 of 8,000 requests, no duplicates, even though Caddy rotated and deleted its oldest files itself |
| **Rotated-file cleanup** (`--remove-rotated true`) | ✅ Files younger than 24 hours stay. Older ones are deleted after verification (3 `.gz`). The active file stays |
| **Cleanup with an unreadable rotated file** | ✅ Nothing is deleted, and the source keeps reading the active file (150 of 150 lines). The error stays visible on the card |
| **systemd service**: `service install --start` | ✅ User unit written, enabled and started |
| **systemd service**: `kill -9` of the process | ✅ Restarted by systemd after 5.2 s, 3,000 of 3,000 lines |
| **systemd service**: `systemctl --user restart` | ✅ Clean stop with a final read, new start in 0.2 s |
| **systemd service**: `check` | ✅ Exit code 0 while running. Exit code 1 with "Loglem non è in esecuzione" one minute after stopping |
| **systemd service**: web page served | ✅ HTTP 200 |
| **systemd service**: `service uninstall` | ✅ Unit removed, data untouched |
| **Backup**: `backup`, `backup-verify`, `restore` | ✅ Copy in 0.07 s. Verification: exit code 0 when intact, 1 on a tampered backup. Restore with 8,021 of 8,021 lines |
| `self-check` | ✅ Executable, SQLite schema and 17 recipes |

### Memory and timings

The Loglem process stayed **between 6 and 9 MB RSS in every test**. The larger numbers belong to the external programs Loglem runs.

| Situation | Loglem | External programs |
|---|---|---|
| Collection from three journal sources | 7.4 MB | `journalctl` 22–48 MB RSS, of which about 1 MB anonymous. The rest is journal files read through the disk cache, which is shared and reclaimable by the kernel |
| Collection from Podman sources | 6.7 MB | `podman` 68 MB while reading |
| Import of the whole journal (566,000 entries) | 8.7 MB peak, 91 s | `journalctl` 147 MB peak |
| `serve` under the Caddy load | 7.4 MB peak, 2.1 MB anonymous | — |
| `serve` for 60 s on the journal archive, status requested every 2 s | 6.4 MB RSS, 0.8 MB anonymous, PSS 4.5 MB | — |
| systemd service (cgroup accounting) | `MemoryCurrent` 9.1 MiB, `MemoryPeak` 11.7 MiB | includes child processes and the cache of written files |

Search on the whole-journal archive (566,000 entries, 100 results):

| Search | Time |
|---|---:|
| Status (`/api/status`), median of 30 requests | 1.5 ms |
| Text over the whole history (present or absent) | 0.32 s |
| Case-insensitive text | 0.61 s |
| Errors only | 1.21 s |
| Absent text, last 7 days | 0.04 s |

Space: the whole-journal archive takes 49 MB, against 586 MB for the journal itself. Archives created by the previous version (137.9 MB for the same 566,000 entries) are brought to the new format on first open: 3.6 s, no line lost, 49 MB. After the migration deduplication still works (no duplicate lines on a new collection) and expiry removes keys and lines together (75,929 lines and 75,929 keys after a 30-day expiry, SQLite integrity `ok`).

