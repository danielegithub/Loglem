# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**One place for your server's logs: where they are, how much space they take, and how long they are kept.**

Loglem reads the logs of your Linux server (`.log` files and Podman containers), copies them into a searchable archive, and automatically deletes archived entries once the retention period you chose has passed. You manage everything from a browser page on the server itself or from a keyboard-driven terminal menu, without editing configuration files.

> **Status: experimental.** It works and is tested, but there is no published release yet and some parts still need end-to-end checks on a real server. See [Limitations](docs/limitations.md).

## Contents

- [Why Loglem](#why-loglem)
- [What it does](#what-it-does)
- [Quick start](#quick-start)
- [Install](#install)
- [Documentation](#documentation)
- [License](#license)

## Why Loglem

Logs pile up in many places: a folder for the web server, another for the app, the output of each container. Nobody remembers where they are, how big they have grown, or when they should be deleted.

With Loglem:

- **You always know where the logs are.** One list shows every source, the original location, the archive location, the size and the state of the last collection.
- **You decide how long each source is kept.** Set 28 days, 6 months or 2 years per source. Expired entries are removed for you, so the disk does not fill up and old data does not stay around by accident.
- **You can find things.** Search text and date ranges across everything that was collected, even after the original file was rotated or the container was removed.
- **It stays out of the way.** A single executable of about 4 MB, around 10 MB of RAM while idle, no database server, no account and no cloud service. Your logs never leave the machine.
- **It is safe by default.** Original files are only read, never modified. Deleting an original is optional and happens only for files you explicitly hand over as finished.
- **It is simple to operate.** Everything is done from a page that listens only on the server itself, or from a terminal menu that works over SSH. Commands for scripts and automation are available too.
- **It replaces a pile of scripts.** Collecting, cleaning up, keeping and searching logs usually means a mix of rotation rules, scheduled jobs and ad-hoc commands. Loglem does it in one place, with the same settings in the web page, the terminal and the command line.
- **It moves with you.** Paths, containers and durations are settings, not code: export the configuration and import it on another machine.

## What it does

1. **You add a source:** a folder of `.log` files, a single file, a Podman container, or the access log of Caddy.
2. **Loglem collects new lines** every 10 seconds by default and stores them in a small SQLite archive for that source. It remembers where it stopped, so nothing is read twice.
3. **You search and check** from the web page or the terminal, and see the state of each source.
4. **Old entries are removed** when they pass the retention you set for that source.
5. **If something goes wrong,** Loglem's own errors are saved with their cause and are visible in a Diagnostics page. A failing source never stops the others.

It runs as a background service (a systemd user service) or on demand. Runs on Linux, without a graphical desktop.

## Quick start

After [installing](#install):

```sh
# Open the guided terminal menu (arrow keys, Enter, Escape)
loglem

# Or add a folder of logs from the command line and read it once
loglem source add-folder --name web-logs --path /srv/example/logs --days 28
loglem collect
loglem status

# Search
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16

# Open the web page (it prints its own address, for example http://127.0.0.1:53142)
loglem web
```

To keep collecting after you close the terminal:

```sh
loglem service install --start
```

The terminal menu and web page currently use Italian labels. Details in [Using Loglem](docs/usage.md).

## Install

When a release is published, download the package for your machine from the **Releases** tab of this repository:

| Machine | Package suffix |
|---|---|
| Linux Intel/AMD 64-bit | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Packages need glibc 2.39 or later (for example Ubuntu 24.04).

Before installing, check the signature with the publisher's public key (`release-public.pem`), obtained through an independent channel:

```sh
openssl pkeyutl -verify -pubin -inkey release-public.pem -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig
sha256sum --check --ignore-missing SHA256SUMS
```

Then install:

```sh
tar xzf loglem-*.tar.gz
cd loglem-*
sh install.sh
~/Loglem/loglem
```

Everything lives in `~/Loglem`: the program and a `data/` folder with settings, archives and diagnostics. The installer does not enable any service on its own. For Podman sources, Podman must be installed and usable by the same user.

To build it yourself, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Documentation

The documentation is available in: [English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Page | What is in it |
|---|---|
| [Using Loglem](docs/usage.md) | Step-by-step guide: add sources, search, retention, running as a service, cleanup, moving to another machine, troubleshooting, updates |
| [Command reference](docs/cli.md) | All commands |
| [Limitations](docs/limitations.md) | What it does not do or guarantee |

## License

[MIT](LICENSE).
