#!/usr/bin/env bash

set -euo pipefail

scie_lift="$1"
file_name="$2"
url="$(jq -r ".get[\"${file_name}\"]" "${scie_lift}")"
exec curl -fL "${url}"