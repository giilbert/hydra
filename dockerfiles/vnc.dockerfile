FROM debian:buster

RUN apt-get update
# Install VNC server and xfce4 desktop environment
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y x11vnc xvfb
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y fluxbox
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y novnc python3-websockify
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y procps net-tools

# Clean-up apt redundant information
RUN apt-get autoremove && apt-get clean && rm -rf /var/lib/apt/lists/*

# Create X11 socket directory
RUN install -d -m 1777 /tmp/.X11-unix
COPY entrypoint.sh /

EXPOSE 6080
ENTRYPOINT ["sh", "/entrypoint.sh"]
