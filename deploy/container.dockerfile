FROM rust:1.70-slim-buster as builder

WORKDIR /usr/src/hydra

RUN apt-get update \
    && apt-get install -y pkg-config libssl-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir crates \
    && cargo new --bin crates/hydra-container \
    && cargo new --bin crates/hydra-server \
    && cargo new --bin crates/hydra-proxy \
    && cargo new --lib crates/shared

COPY Cargo.toml Cargo.lock ./

# Copy the Cargo.toml files into the image and compile only the dependencies
# storing them in a layer that we can cache and reuse
COPY crates/hydra-container/Cargo.toml crates/hydra-container/Cargo.toml
COPY crates/hydra-server/Cargo.toml crates/hydra-server/Cargo.toml
COPY crates/hydra-proxy/Cargo.toml crates/hydra-proxy/Cargo.toml
COPY crates/shared/Cargo.toml crates/shared/Cargo.toml
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/hydra/target \
    cargo build --release --bin hydra-container

COPY crates/shared crates/shared
COPY crates/hydra-container crates/hydra-container

RUN touch crates/hydra-container/src/main.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/hydra/target \
    cargo build --release --package hydra-container \
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
