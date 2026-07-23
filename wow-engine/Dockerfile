# Build with a complete Rust toolchain, then copy only the release artifact into
# the runtime image. Keeping the build and runtime stages separate prevents
# compilers, package managers, and source code from reaching production.
FROM rust:1.76-slim-bookworm AS builder

WORKDIR /usr/src/wow-engine

# reqwest uses the system trust store at runtime. Install the certificate
# bundle here so the exact same bundle can be copied into the distroless image.
RUN apt-get update \
    && apt-get install --no-install-recommends --yes binutils ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy dependency manifests and source from the `wow-engine` build context.
# Keeping the manifests separate also allows Docker to cache dependency builds.
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --locked --release \
    && strip target/release/wow-engine

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /usr/src/wow-engine/target/release/wow-engine /usr/local/bin/wow-engine
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

ENV PORT=8080 \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/wow-engine"]