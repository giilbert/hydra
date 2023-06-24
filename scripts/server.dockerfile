FROM rust:1.70 as builder

WORKDIR /usr/src/hydra

ADD Cargo.toml Cargo.toml
ADD hydra-container/Cargo.toml hydra-container/Cargo.toml
ADD hydra-server/Cargo.toml hydra-server/Cargo.toml
ADD protocol protocol

RUN --mount=type=cache,target=/usr/src/hydra/target mkdir hydra-container/src hydra-server/src \
    && touch hydra-container/src/main.rs hydra-server/src/main.rs \
    && echo "fn main() {}" > hydra-container/src/main.rs \
    && echo "fn main() {}" > hydra-server/src/main.rs \
    && cargo build --release --bin hydra-server \
    && rm -rf hydra-container hydra-server

COPY . .

# update mtime to force rebuild, and then build after building dependencies and caching them
RUN --mount=type=cache,target=/usr/src/hydra/target touch hydra-server/src/main.rs \
    && cargo build --release --bin hydra-server \
    && mv target/release/hydra-server /bin/hydra-server

# hydra-server
FROM debian:latest
RUN apt update \
    && apt install -y libssl-dev ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /bin/hydra-server /bin/hydra-server
ADD test-certs /etc/hydra/test-certs
ENV RUST_LOG=info
WORKDIR /etc/hydra
CMD ["/bin/hydra-server"]
