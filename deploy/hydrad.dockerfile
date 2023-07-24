FROM rust:1.70-slim-buster as builder

WORKDIR /usr/src/hydra

RUN apt-get update \
    && apt-get install -y pkg-config libssl-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

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
    cargo build --release --bin hydrad

COPY crates/shared crates/shared
COPY crates/hydrad crates/hydrad

RUN touch crates/hydrad/src/main.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/hydra/target \
    cargo build --release --package hydrad \
    && mv target/release/hydrad /bin/hydrad

# hydrad
FROM debian:buster-slim
RUN apt-get update \
    && apt-get install -y python3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /bin/hydrad /bin/hydrad
ENV ENVIRONMENT=production
CMD ["/bin/hydrad"]
