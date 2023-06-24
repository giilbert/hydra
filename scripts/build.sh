#!/bin/bash

C_RESET=$(tput sgr0)

C_RED=$(tput setaf 1)
C_BLUE=$(tput setaf 4)
C_PURPLE=$(tput setaf 5)

ROOT="$(git rev-parse --show-toplevel)"

echo ">>> ${C_PURPLE}Hydra${C_RESET} - build script"
echo " |- Commit $(git log -1 --format='%h')"
echo " |- $(docker --version)"

echo ""

# ---------- Setup ----------

echo ">>> ${C_PURPLE}Setup${C_RESET}"
echo "${C_BLUE}\$${C_RESET} cd ${ROOT}"
cd ${ROOT}
echo ""

# ---------- Build ----------

echo ">>> ${C_PURPLE}Building${C_RESET} hydra-server (this will take a while)"
echo "${C_BLUE}\$${C_RESET} docker build --file ./scripts/server.dockerfile -t hydra-server ."
docker build --file ./scripts/server.dockerfile -t hydra-server .

echo ">>> ${C_PURPLE}Building${C_RESET} hydra-container (this will also take a while)"
echo "${C_BLUE}\$${C_RESET} docker build --file ./scripts/container.dockerfile -t hydra-container ."
docker build --file ./scripts/container.dockerfile -t hydra-container .

# ---------- Goodbye ----------
echo ""

echo ">>> ${C_PURPLE}Done!${C_RESET}"
echo " |- ${C_PURPLE}hydra-server${C_RESET} and ${C_PURPLE}hydra-container${C_RESET} built successfully"
echo " |- Run in development with ${C_BLUE}./dev.sh${C_RESET} in the scripts directory"
echo " |- Would you like to run it now? (y/n)"
read option

if [ "$option" = "y" ]; then
  echo "${C_BLUE}\$${C_RESET} cd ${ROOT}/scripts"
  echo ""
  cd ${ROOT}/scripts
  echo "${C_BLUE}\$${C_RESET} chmod +x ./dev.sh"
  echo ""
  chmod +x ./dev.sh
  echo "${C_BLUE}\$${C_RESET} ./dev.sh"
  echo ""
  ./dev.sh
fi
