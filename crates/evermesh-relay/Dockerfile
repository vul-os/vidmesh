# evermesh-relay container image.
#
# evermesh-relay depends on evermesh-kernel via a workspace path
# dependency, so the build context for this Dockerfile MUST be the
# repository root, not this crate directory, e.g.:
#
#   docker build -f crates/evermesh-relay/Dockerfile -t evermesh-relay .
#
# (docker-compose.yml at the repo root does this already.)

FROM rust:1-bookworm AS build
WORKDIR /workspace

# Workspace manifests and lockfile first, for layer caching.
COPY Cargo.toml Cargo.lock ./
COPY crates/evermesh-kernel ./crates/evermesh-kernel
COPY crates/evermesh-relay ./crates/evermesh-relay
COPY crates/evermesh-wasm ./crates/evermesh-wasm
COPY crates/evermesh-node ./crates/evermesh-node

RUN cargo build --release -p evermesh-relay

FROM debian:bookworm-slim AS runtime

# ca-certificates: gossip peers are typically wss:// (TLS); wget: used
# by the compose healthcheck against GET /info.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /workspace/target/release/evermesh-relay /usr/local/bin/evermesh-relay

ENV EVERMESH_RELAY_CONFIG=/etc/evermesh/relay.json
VOLUME ["/data"]
EXPOSE 8787

ENTRYPOINT ["/usr/local/bin/evermesh-relay"]
