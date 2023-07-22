#!/bin/bash

C_RESET=$(tput sgr0)

C_RED=$(tput setaf 1)
C_BLUE=$(tput setaf 4)
C_PURPLE=$(tput setaf 5)

ROOT="$(git rev-parse --show-toplevel)"

echo ">>> ${C_PURPLE}Hydra${C_RESET} - dev script"
echo " |- Commit $(git log -1 --format='%h')"
echo " |- $(docker --version)"
echo ""

# check that a /tmp/hydra exists
if [ ! -d "/tmp/hydra" ]; then
  echo ">>> ${C_PURPLE}Setup${C_RESET}"
  echo "${C_BLUE}\$${C_RESET} mkdir /tmp/hydra"
  echo ""
  mkdir /tmp/hydra
fi

echo ">>> ${C_PURPLE}Running${C_RESET} hydra-server"
echo "${C_BLUE}\$${C_RESET} cd ${ROOT}"
echo ""
cd ${ROOT}

echo "${C_BLUE}\$${C_RESET} docker compose --file dev/hydra-development.yml up"
docker compose --file dev/hydra-development.yml up
