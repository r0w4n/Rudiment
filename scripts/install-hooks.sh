#!/bin/sh
set -eu

repository_root=$(git rev-parse --show-toplevel)
cd "$repository_root"

chmod +x .githooks/pre-commit .githooks/pre-push
git config core.hooksPath .githooks
echo "Rudiment Git hooks installed from .githooks/"
