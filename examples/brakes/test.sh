# Copyright 2026 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

check_cmd mktemp rm rmdir

gc "${PWD}/brakes"
stderr="$("${SCIE_JUMP}" "${LIFT}" 2>&1 >/dev/null)"
if [[ "${stderr}" =~ "WARN" ]] && \
    [[ "${stderr}" =~ "Scie boot binding descriptions are deprecated; ignoring: " ]] && \
    [[ "${stderr}" =~ "Stress deprecation of scie boot binding descriptions." ]]; then
  log "Observed expected warning: ${stderr}"
else
  die "Did not observe expected warning in STDERR: ${stderr}"
fi

BIND_BASE_DIR="$(mktemp -d)"
gc "${BIND_BASE_DIR}"
export BIND_BASE_DIR

export RUST_LOG=debug

for bind_json in 0 1; do
  echo
  if [ "${bind_json}" = "0" ]; then
    echo "Using traditional SCIE_BINDING_ENV file bindings."
  else
    echo "Using SCIE_BINDING_JSON file bindings."
  fi
  echo "==="

  export BIND_JSON="${bind_json}"

  rm -rf "${SCIE_BASE}" "${BIND_BASE_DIR}"
  ./brakes && echo "Initial run created bindings"

  echo -e "\n---"
  rm -rf "${BIND_BASE_DIR}"
  ./brakes && echo "Second run re-created bindings due to missing dir."

  echo -e "\n---"
  rmdir "${BIND_BASE_DIR}/dir"
  ./brakes && echo "Third run re-created bindings due to missing dir."

  echo -e "\n---"
  rm "${BIND_BASE_DIR}/file"
  ./brakes && echo "Fourth run re-created bindings due to missing file."

  echo -e "\n---"
  rm "${BIND_BASE_DIR}/exists"
  ./brakes && echo "Fifth run re-created bindings due to path not existing."

  echo -e "\n---"
  ./brakes && echo "Sixth run re-used unbroken bindings from fifth run."
done