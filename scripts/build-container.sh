#!/bin/bash

C_RESET=$(tput sgr0)

C_RED=$(tput setaf 1)
C_BLUE=$(tput setaf 4)
C_PURPLE=$(tput setaf 5)

ROOT="$(git rev-parse --show-toplevel)"

echo ">>> ${C_PURPLE}Hydra${C_RESET} - hydra-container build script"
echo " |- Commit $(git log -1 --format='%h')"
echo " |- $(docker --version)"

echo ""

# ---------- Setup ----------

echo ">>> ${C_PURPLE}Setup${C_RESET}"
echo "${C_BLUE}\$${C_RESET} cd ${ROOT}"
cd ${ROOT}
echo ""

# ---------- Build ----------

echo ">>> ${C_PURPLE}Building${C_RESET} hydra-container (this will also take a while)"
echo "${C_BLUE}\$${C_RESET} docker build --file ./deploy/container.dockerfile -t hydra-container ."
docker build --file ./deploy/container.dockerfile -t hydra-container .

echo ">>> ${C_PURPLE}Saving${C_RESET} hydra-container to images/hydra-container.tar"

# check that there is a directory called images
if [ ! -d "images" ]; then
  echo "${C_BLUE}\$${C_RESET} mkdir images"
  mkdir images
fi

echo "${C_BLUE}\$${C_RESET} docker save hydra-container > images/hydra-container.tar"
docker save hydra-container > images/hydra-container.tar

