# Multi-stage build for `gho` — minimal Alpine runtime (~5 MiB)
#
# Build:    docker build -t ghcr.io/tomeq82/gho:latest .
# Run:      docker run --rm -v "$PWD:/data" ghcr.io/tomeq82/gho info /data/backup.gho

FROM rust:1.85-alpine AS builder

# Alpine needs musl-dev for Rust static linking, and ca-certificates for HTTPS.
RUN apk add --no-cache musl-dev pkgconfig

WORKDIR /build

# Cache dependencies separately from the source.
COPY Cargo.toml Cargo.lock* ./
COPY src ./src

RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release --locked && \
    rm -rf src/main.rs target/release/deps/gho-*

# Real build.
COPY src/main.rs ./src/main.rs
RUN cargo build --release --locked --bin gho

# ----------------------------------------------------------------------------
# Runtime image — Alpine + ca-certificates (~9 MiB)
# ----------------------------------------------------------------------------
# Note: We deliberately don't run as nonroot. `gho` needs to read arbitrary
# backup files supplied via bind-mount, which are usually owned by the host
# user with mode 0600. Use `--user "$(id -u):$(id -g)"` at run time if you
# need to drop privileges.
FROM alpine:3.21

# ca-certificates enables HTTPS for cosign signature verification / future
# remote fetches. tini gives proper signal handling.
RUN apk add --no-cache ca-certificates tini

COPY --from=builder /build/target/release/gho /usr/local/bin/gho

ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/gho"]
CMD ["--help"]
