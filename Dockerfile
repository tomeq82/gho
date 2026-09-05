# Multi-stage build for `gho` — minimal runtime image (~10 MiB)
#
# Build:    docker build -t ghcr.io/tomeq82/gho:latest .
# Run:      docker run --rm -v "$PWD:/data" ghcr.io/tomeq82/gho info /data/backup.gho

FROM rust:1.85-slim AS builder

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
# Runtime image — scratch + ca-certificates for cosign signature verification
# and an unprivileged user for extract output files.
# ----------------------------------------------------------------------------
FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=builder /build/target/release/gho /usr/local/bin/gho

# Sanity check
ENTRYPOINT ["/usr/local/bin/gho"]
CMD ["--help"]
