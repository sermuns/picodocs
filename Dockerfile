# based on https://github.com/jdx/mise/blob/main/packaging/mise/Dockerfile

FROM rust AS builder

LABEL maintainer="sermuns"
LABEL org.opencontainers.image.source=https://github.com/sermuns/picodocs
LABEL org.opencontainers.image.description="extremely tiny and fast alternative to MkDocs"
LABEL org.opencontainers.image.licenses=AGPL-3.0-only

WORKDIR /work
COPY . /work/

RUN cargo build --release

FROM scratch

COPY --from=builder /work/target/release/picodocs /bin/picodocs
ENTRYPOINT ["picodocs"]
