# vidmesh-relay container image.
#
# vidmesh-relay depends on vidmesh-kernel via a workspace path
# dependency, so the build context for this Dockerfile MUST be the
# repository root, not this crate directory, e.g.:
#
#   docker build -f crates/vidmesh-relay/Dockerfile -t vidmesh-relay .
#
# (docker-compose.yml at the repo root does this already.)

FROM rust:1-bookworm AS build
WORKDIR /workspace

# Workspace manifests and lockfile first, for layer caching.
COPY Cargo.toml Cargo.lock ./
COPY crates/vidmesh-kernel ./crates/vidmesh-kernel
COPY crates/vidmesh-relay ./crates/vidmesh-relay
COPY crates/vidmesh-wasm ./crates/vidmesh-wasm
COPY crates/vidmesh-node ./crates/vidmesh-node

RUN cargo build --release -p vidmesh-relay

FROM debian:bookworm-slim AS runtime

# ca-certificates: gossip peers are typically wss:// (TLS); wget: used
# by the compose healthcheck against GET /info.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /workspace/target/release/vidmesh-relay /usr/local/bin/vidmesh-relay

ENV VIDMESH_RELAY_CONFIG=/etc/vidmesh/relay.json
VOLUME ["/data"]
EXPOSE 8787

ENTRYPOINT ["/usr/local/bin/vidmesh-relay"]
