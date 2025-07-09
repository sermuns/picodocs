> [!NOTE]
> This project does not yet exist. The README below is speculative, stay tuned!

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

I love how documentation with MkDocs is a very non-intrusive thing to add to your existing codebase: just a `docs/` directory for the markdown content for configuration

picodocs takes great inspiration from projects such as [zola](https://github.com/getzola/zola), [MkDocs](https://github.com/mkdocs/mkdocs) and [mdBook](https://github.com/rust-lang/mdBook), though it has a few key differences:

- _Like_ zola and mdBook, picodocs written in Rust, which makes it very fast and small. It is also shipped as a single binary, so you can easily run it on any platform without worrying about dependencies.

- _Unlike_ mdBook, a rendered picodocs site rendered site not split into numbered chapters, like a book. It is more similar to MkDocs, where the documentation is split into pages, which can be linked to each other.

- _Like_

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
