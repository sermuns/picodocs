<a href="https://github.com/sermuns/picodocs">
  <img src="docs/banner.png">
</a>

<div align="center">
  <p>
  <em>
      mkdocs, but very fast and small
  </em>
  </p>
  <a href="https://github.com/sermuns/picodocs/releases/latest">
    <img alt="release-badge" src="https://img.shields.io/github/v/release/sermuns/picodocs.svg"></a>
  <a href="https://github.com/sermuns/picodocs/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-%20%20GNU%20GPLv3%20-green"></a>
  <a href="https://crates.io/crates/picodocs"><img src="https://img.shields.io/crates/v/picodocs.svg"></a>
</div>

---

picodocs is a simple, fast and small static site generator for documentation written in Markdown.

picodocs is inspired by [zola](https://github.com/getzola/zola), [MkDocs](https://github.com/mkdocs/mkdocs) and [mdBook](https://github.com/rust-lang/mdBook), though it

I like and

I love how documentation with MkDocs is a very non-intrusive thing to add to your existing codebase: just a `docs/` directory for the markdown content for configuration

## Installation

### From prebuilt binaries

For each version, prebuilt binaries are automatically built for Linux, MacOS and Windows.

- You can download and unpack the latest release from the [releases page](https://github.com/sermuns/picodocs/releases/latest).

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
