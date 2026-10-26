#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
exec cargo run -p finder "$@"
