FROM rust:1.70-alpine3.18 as builder

WORKDIR /usr/src/hydra

RUN apk add --no-cache musl-dev

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

COPY protocol protocol
COPY hydra-server hydra-server
COPY hydra-container hydra-container
COPY Cargo.toml Cargo.toml

# update mtime to force rebuild, and then build after building dependencies and caching them
RUN --mount=type=cache,target=/usr/src/hydra/target touch hydra-server/src/main.rs \
    && cargo build --release --bin hydra-server \
    && mv target/release/hydra-server /bin/hydra-server

# hydra-server
FROM docker:dind
USER root
RUN apk add --no-cache libressl-dev ca-certificates-bundle tini bash
COPY --from=builder /bin/hydra-server /bin/hydra-server
COPY images /images
COPY ./scripts/server-entrypoint.sh /server-entrypoint.sh
ADD test-certs /etc/hydra/test-certs
ENV RUST_LOG=info
WORKDIR /etc/hydra
RUN chmod +x /server-entrypoint.sh
CMD ["/server-entrypoint.sh"]
