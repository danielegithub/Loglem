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
