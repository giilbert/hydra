FROM hydrad:latest as daemon

FROM debian:buster-slim

RUN apt-get update

RUN apt-get install -y curl \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_18.x | bash - \
    && apt-get install -y nodejs

COPY --from=daemon /bin/hydrad /bin/hydrad

CMD ["/bin/hydrad"]