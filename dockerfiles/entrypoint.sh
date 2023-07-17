#!/bin/sh
Xvfb -screen 0 "512x512x24" -ac &
sleep 5

# REMEMBER TO SET ENVIRONMENT VARIABLE!!!
export DISPLAY=:0.0
x11vnc -noxrecord -noxfixes -noxdamage -forever -display :0 &

bash /usr/share/novnc/utils/launch.sh
