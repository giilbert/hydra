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
    && cargo build --release --bin hydra-container \
    && rm -rf hydra-container hydra-server

COPY . .

# update mtime to force rebuild, and then build after building dependencies and caching them
RUN --mount=type=cache,target=/usr/src/hydra/target touch hydra-container/src/main.rs \
    && cargo build --release --bin hydra-container \
    && mv target/release/hydra-container /bin/hydra-container

# hydra-container
FROM alpine:3.18.2
RUN apk add --no-cache libressl-dev ca-certificates-bundle python3
COPY --from=builder /bin/hydra-container /bin/hydra-container
ENV ENVIRONMENT=production
CMD ["/bin/hydra-container"]
