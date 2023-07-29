FROM hydrad:latest as daemon

FROM debian:buster-slim

RUN apt-get update
# Install VNC server and xfce4 desktop environment
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y x11vnc xvfb
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y fluxbox
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y novnc python3-websockify
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y procps net-tools

# install python3 and turtle
RUN apt-get install -y python3 python3-tk

# Clean-up apt redundant information
RUN apt-get autoremove && apt-get clean && rm -rf /var/lib/apt/lists/*

# Create X11 socket directory
RUN install -d -m 1777 /tmp/.X11-unix
COPY turtle/entrypoint.sh /

COPY --from=daemon /bin/hydrad /bin/hydrad

EXPOSE 6080
ENV DISPLAY=:0.0
ENTRYPOINT ["sh", "/entrypoint.sh"]