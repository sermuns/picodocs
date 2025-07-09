<a href="https://github.com/sermuns/picodocs">
  <img src="docs/banner.png">
</a>

<div align="center">
  <p>
  <em>
      mkdocs, but smaller
  </em>
  </p>
  <a href="https://github.com/sermuns/meread/releases/latest">
    <img alt="release-badge" src="https://img.shields.io/github/v/release/sermuns/picodocs.svg"></a>
  <a href="https://github.com/sermuns/meread/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/License-WTFPL-brightgreen.svg"></a>
  <a href="https://crates.io/crates/picodocs"><img src="https://img.shields.io/crates/v/picodocs.svg"></a>
</div>

---

MEREAD is a command-line tool for previewing Markdown files as they will be presented on GitHub, all completely locally and offline.

## Motivation

I like [MkDocs](https://github.com/mkdocs/mkdocs) and [mdBook](https://github.com/rust-lang/mdBook)

I love how documentation with MkDocs is a very non-intrusive thing to add to your existing codebase: just a `docs/` directory for the markdown content for configuration

## Installation

### From prebuilt binaries

For each version, prebuilt binaries are automatically built for Linux, MacOS and Windows.

- You can download and unpack the
  latest release from the [releases page](https://github.com/sermuns/picodocs/releases/latest).

- Using [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall)

  ```bash
  cargo binstall picodocs
  ```

### From source

- ```bash
  cargo install picodocs
  ```

- ```bash
  git clone https://github.com/sermuns/picodocs
  cd picodocs
  cargo install
  ```
