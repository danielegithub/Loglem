# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**One place for your server's logs: where they are, how much space they take, and how long they are kept.**

Loglem reads your logs, copies them into a searchable archive, and deletes the copies when the time you chose has passed. You use a web page or a terminal menu. No configuration files.

> **Status: experimental.** It works and is tested on macOS. There is no published release yet, and some parts are not tested on real Linux. See [Limitations](docs/limitations.md).

## Contents

- [Why Loglem](#why-loglem)
- [What it reads](#what-it-reads)
- [Safe cleanup of original files](#safe-cleanup-of-original-files)
- [Quick start](#quick-start)
- [Install](#install)
- [Documentation](#documentation)
- [License](#license)

## Why Loglem

Logs pile up in many places. Nobody remembers where they are, how big they are, or when to delete them.

- **You see everything.** One list: every source, where it is, the size, the state.
- **You choose how long.** 28 days, 6 months, 2 years, per source. Old copies disappear by themselves.
- **You can search.** Text, dates, IP, path, status code. Also after the file was rotated or the container removed. New lines appear live.
- **It stays small.** One program of about 5 MB. About 8 MB of RAM at rest (measured on Linux). No database server, no account, no cloud. Your logs stay on your machine.
- **It is safe by default.** Original files are only read. Deleting them is optional and checked (see below).
- **It is simple.** A web page, a terminal menu that works over SSH, and commands for scripts.
- **It shows what went wrong.** A stopped source, a gap in the logs, a full disk: you see it, and `loglem check` tells your monitor.
- **It helps with personal data.** Mask IPs, erase or export one person's lines, see who looked at the logs. See [privacy](docs/privacy.md).

## What it reads

| Source | Notes |
|---|---|
| Folders and files of `.log` text | Also rotated files, also `.gz` (history is imported once, never duplicated). |
| Caddy, Nginx, Apache | Found automatically in their configuration. |
| Podman, Docker | A container, or by name through the journal. |
| Kubernetes | One pod, or all pods of an application. |
| Linux journal | All of it, or some services, programs or severities. |

## Safe cleanup of original files

Off by default. When you turn it on for a source, Loglem deletes a finished file only after this check:

1. It **copies** the file into its archive.
2. It **checks the copy against the original, byte by byte**. If a line is missing, expired or changed, it stops.
3. It waits until nobody has changed the file for **24 hours**.
4. Only then it deletes the original.

The file a program is still writing to is never touched. If any step fails, the original stays where it is. Works for Caddy (rotated files) and for folders of finished files.

## Quick start

After [installing](#install):

```sh
loglem                                  # terminal menu
loglem discover                         # find Caddy, Nginx and Apache logs
loglem source add-folder --name web --path /srv/example/logs --days 28
loglem collect
loglem search --source web --text '/login' --from 2026-11-01
loglem web                              # collects and opens the web page (it prints the address)
```

To keep running after you close the terminal (Linux: systemd, macOS: launchd):

```sh
loglem service install --start
```

The terminal menu and the web page use Italian labels. More in [Using Loglem](docs/usage.md).

## Install

When a release is published, download the package for your machine from the **Releases** tab:

| Machine | Package suffix |
|---|---|
| Linux Intel/AMD 64-bit | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Packages need glibc 2.39 or later (for example Ubuntu 24.04).

Check the signature with the publisher's public key (`release-public.pem`), obtained through an independent channel:

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

Everything lives in `~/Loglem`: the program and a `data/` folder. The installer does not start any service. To build it yourself, see [CONTRIBUTING.md](CONTRIBUTING.md).

**In a container.** Build it with `podman build -t loglem -f Containerfile .` and run it with a folder of logs mounted. See [Using Loglem](docs/usage.md#11-in-a-container).

## Documentation

[English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Page | What is in it |
|---|---|
| [Using Loglem](docs/usage.md) | Short steps for everything |
| [Commands](docs/cli.md) | All commands |
| [Personal data and proof mode](docs/privacy.md) | GDPR tools, erase, export, audit, sealed days |
| [If Loglem stops](docs/stopping.md) | What you lose, and what Loglem does |
| [Limitations](docs/limitations.md) | What it does not do or promise |

## License

[MIT](LICENSE).
