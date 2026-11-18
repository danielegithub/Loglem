# Contributing

Loglem is written in Rust (1.88 or newer). You also need a C compiler; SQLite is built into the program.

```sh
cargo build --release     # the program is target/release/loglem
cargo test --all-targets  # run the tests
```

## Add a program

Loglem knows programs through small JSON files in `src/recipes/`, with no Rust code:

1. Copy a recipe close to yours (for example `src/recipes/nginx.json`) to `src/recipes/<id>.json`; `id` must match the file name.
2. Change name, `categories`, the typical log paths (`logs`, globs allowed), how to recognize it (`recognize`: processes, config files, container images, systemd units) and, if useful, `journal` and `config`.
3. Write the `notes` in every language of `src/i18n/` (today `it` and `en`). Never suggest keeping logs longer than the user chooses in Loglem.
4. Run `cargo test` and `loglem self-check`: they check every recipe.
5. Open a pull request.

`config.reader` is `directives` (lines like `access_log /path;` or `key = value`, with site blocks), or one of the small readers for special formats listed in `src/recipes.rs`.

Please open an issue before starting a large change. Describe what you want to do and why, so it can be discussed first.

By contributing you agree that your work is released under the [MIT license](LICENSE).
