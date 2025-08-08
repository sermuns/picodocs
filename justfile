build-release:
  cargo build --release

benchmark: build-release
    hyperfine \
      --shell=none \
      --warmup 200 \
      'target/release/picodocs build'
