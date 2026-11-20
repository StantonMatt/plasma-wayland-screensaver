#!/usr/bin/env bash
# Install CI packages through a download directory that the workflow caches
# between runs, so a slow mirror only costs new or updated packages. apt checks
# every cached file against the signed package index before using it.
set -euo pipefail

archives=${APT_ARCHIVES:?set APT_ARCHIVES to the cached download directory}
mkdir -p "$archives/partial"
options=(-o "Dir::Cache::Archives=$archives" -o APT::Keep-Downloaded-Packages=true)

sudo apt-get update
sudo apt-get install --yes --no-install-recommends "${options[@]}" "$@"
# Drop superseded downloads and the root-only lock so the cache stays small
# and readable when the workflow saves it.
sudo apt-get autoclean "${options[@]}"
sudo rm -rf "$archives/lock" "$archives/partial"
