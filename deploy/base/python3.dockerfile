FROM hydrad:latest as daemon

FROM debian:buster-slim

RUN apt-get update

RUN apt-get install -y python3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=daemon /bin/hydrad /bin/hydrad

CMD ["/bin/hydrad"]