FROM hydrad:latest

RUN apt-get update

RUN apt-get install -y python3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

CMD ["/bin/hydrad"]