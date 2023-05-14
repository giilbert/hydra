FROM rust:1.66 as builder

WORKDIR /usr/src
RUN USER=root cargo new hydra-container

WORKDIR /usr/src/hydra-container

RUN USER=root cargo new protocol --lib

COPY Cargo.toml ./

RUN cargo build --release \
    && rm -rf src protocol

ADD . .
# update mtimes https://github.com/rust-lang/cargo/issues/7181#issuecomment-515260460
RUN touch src/main.rs

RUN cargo build --release \
    && mv target/release/hydra-container /bin \
    && rm -rf /usr/src/hydra-container

FROM debian:latest
RUN apt update \
    && apt install -y libssl-dev ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /bin/hydra-container /bin/hydra-container

ENV RUST_LOG=info

CMD ["/bin/hydra-container"]
