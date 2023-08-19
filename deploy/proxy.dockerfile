FROM rust:1.70-alpine3.18 as builder

WORKDIR /usr/src/hydra

RUN apk add --no-cache musl-dev

RUN mkdir crates \
    && cargo new --bin crates/hydrad \
    && cargo new --bin crates/hydra-server \
    && cargo new --bin crates/hydra-proxy \
    && cargo new --lib crates/shared

COPY Cargo.toml Cargo.lock ./

# Copy the Cargo.toml files into the image and compile only the dependencies
# storing them in a layer that we can cache and reuse
COPY crates/hydrad/Cargo.toml crates/hydrad/Cargo.toml
COPY crates/hydra-server/Cargo.toml crates/hydra-server/Cargo.toml
COPY crates/hydra-proxy/Cargo.toml crates/hydra-proxy/Cargo.toml
COPY crates/shared/Cargo.toml crates/shared/Cargo.toml
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/hydra/target \
    cargo build --release --bin hydra-proxy

COPY crates/shared crates/shared
COPY crates/hydra-proxy crates/hydra-proxy

RUN touch crates/hydra-proxy/src/main.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/hydra/target \
    cargo build --release --package hydra-proxy \
    && mv target/release/hydra-proxy /bin/hydra-proxy

# hydra-proxy
FROM alpine:3.18

RUN apk add --no-cache libressl-dev ca-certificates-bundle bash ncurses

WORKDIR /etc/hydra

ENV RUST_LOG=info
ENV RUST_BACKTRACE=1
EXPOSE 3101

COPY --from=builder /bin/hydra-proxy /bin/hydra-proxy

CMD ["/bin/hydra-proxy"]
