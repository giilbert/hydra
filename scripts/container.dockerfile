FROM rust:1.70-slim-buster as builder

WORKDIR /usr/src/hydra

RUN apt-get update \
    && apt-get install -y pkg-config libssl-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

ADD Cargo.toml Cargo.toml
ADD hydra-container/Cargo.toml hydra-container/Cargo.toml
ADD hydra-server/Cargo.toml hydra-server/Cargo.toml
ADD hydra-proxy/Cargo.toml hydra-proxy/Cargo.toml
ADD protocol protocol

RUN --mount=type=cache,target=/usr/src/hydra/target mkdir hydra-container/src hydra-server/src hydra-proxy/src \
    && touch hydra-container/src/main.rs hydra-server/src/main.rs hydra-proxy/src/main.rs \
    && echo "fn main() {}" > hydra-container/src/main.rs \
    && echo "fn main() {}" > hydra-server/src/main.rs \
    && echo "fn main() {}" > hydra-proxy/src/main.rs \
    && cargo build --release --bin hydra-container \
    && rm -rf hydra-container hydra-server

COPY . .

# update mtime to force rebuild, and then build after building dependencies and caching them
RUN --mount=type=cache,target=/usr/src/hydra/target touch hydra-container/src/main.rs \
    && cargo build --release --bin hydra-container \
    && mv target/release/hydra-container /bin/hydra-container

# hydra-container
FROM debian:buster-slim
RUN apt-get update \
    && apt-get install -y python3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /bin/hydra-container /bin/hydra-container
ENV ENVIRONMENT=production
CMD ["/bin/hydra-container"]
