#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
mkdir -p man
pandoc --standalone --to man CLI-GUIDE.md --output man/somme.1
