#!/bin/bash

C_RESET=$(tput sgr0)

C_RED=$(tput setaf 1)
C_BLUE=$(tput setaf 4)
C_PURPLE=$(tput setaf 5)

echo ">>> ${C_PURPLE}Hydra${C_RESET} - server entrypoint script"
echo " |- $(docker --version)"

echo ""

# Start docker service in background
echo "> ${C_PURPLE}Booting${C_RESET} - Spawned dockerd"
dockerd > /dev/null 2>&1 &

# Wait until the docker service is up
while ! docker ps; do
  echo "> ${C_PURPLE}Booting${C_RESET} - Waiting on dockerd to be ready"
  sleep 1
done

echo "> ${C_PURPLE}Booting${C_RESET} - dockerd ready -> importing images"

# Import pre-installed images
for file in /images/*.tar; do
  echo "> ${C_PURPLE}Booting${C_RESET} - Importing image $file"
  docker load -q <$file
done

/bin/hydra-server