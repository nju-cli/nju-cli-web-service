#!/usr/bin/env bash
set -euo pipefail

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
