#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Linux" ]; then
  echo "Docker sandbox images must be built from Linux. Run this script inside Linux or Orb." >&2
  exit 1
fi

image_tar="$(nix build --print-out-paths .#dockerImage)"
socket="${DOCKER_SOCKET:-/var/run/docker.sock}"

curl \
  --fail \
  --silent \
  --show-error \
  --unix-socket "$socket" \
  --header 'Content-Type: application/x-tar' \
  --data-binary "@$image_tar" \
  http://localhost/images/load

echo
echo "Loaded nju-cli-codex-docker:latest into Docker daemon at $socket from $image_tar"
