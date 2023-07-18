#!/bin/sh
Xvfb -screen 0 "512x512x24" -ac &
sleep 5

x11vnc -noxrecord -noxfixes -noxdamage -forever -display :0 &
fluxbox &

bash /usr/share/novnc/utils/launch.sh
