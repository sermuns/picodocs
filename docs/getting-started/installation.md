---
title: Installation
---

# Installation

There are many ways to install picodocs. Pick one you feel comfortable with!

## From prebuilt binaries

For each version, prebuilt binaries are automatically built for Linux, MacOS and Windows.

- You can download and unpack the latest release from the [releases page](https://github.com/sermuns/picodocs/releases/latest).

- Using [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall)
  ```bash
  cargo binstall picodocs
  ```

## From source

- ```bash
  cargo install picodocs
  ```

## Using docker

Images are published to `ghcr.io/sermuns/picodocs`.

Remember to mount your workspace with configuration and docs into the container! It's also more comfortable if you run the container with your user ID and group ID, so that the files created by the container are owned by you, not root.

Examples of using the container:

```shell
docker run -v .:/app -u $(id -u):$(id -g) ghcr.io/sermuns/picodocs build
```

```shell
docker run -p 1809:1809 -v .:/app -u $(id -u):$(id -g) ghcr.io/sermuns/picodocs serve --address 0.0.0.0:1809
```
