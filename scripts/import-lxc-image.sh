#!/usr/bin/env bash
set -euo pipefail

alias_name="${1:-nju-cli-codex-lxc}"
project="${LXC_PROJECT:-nju-cli-web}"

out="$(nix build --print-out-paths .#lxcImage)"
lxc project show "$project" >/dev/null 2>&1 || lxc project create "$project"
lxc image delete "$alias_name" >/dev/null 2>&1 || true
lxc image import "$out" --alias "$alias_name"

echo "Imported $alias_name for project $project from $out"

