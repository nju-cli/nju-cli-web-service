#!/usr/bin/env bash
set -euo pipefail

case "$(uname -m)" in
  arm64 | aarch64)
    target_system="aarch64-linux"
    ;;
  x86_64 | amd64)
    target_system="x86_64-linux"
    ;;
  *)
    echo "Unsupported machine architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

image_tar="$(nix build --print-out-paths ".#packages.${target_system}.dockerImage")"
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
