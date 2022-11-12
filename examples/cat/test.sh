# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

check_cmd cat chmod

"${SCIE_JUMP}" "${LIFT}"
JAVA="java${EXE_EXT}"
gc "${PWD}/${JAVA}"

sha256 "${JAVA}" > "${JAVA}.sha256"
gc "${PWD}/${JAVA}.sha256"

./"${JAVA}" "scie-jump boot-pack"

SCIE_JUMP_SIZE="$(SCIE="inspect" ./"${JAVA}" | jq -r '.scie.jump.size')"
SCIE_JUMP_VERSION="$(SCIE="inspect" ./"${JAVA}" | jq -r '.scie.jump.version')"

# We should be able to assemble an identical scie using cat.
JDK="$(jq -r '.scie.lift.files[0].name' "${LIFT}")"
SCIE_CAT="java.cat${EXE_EXT}"
gc "${PWD}/${SCIE_CAT}"

function scie_cat() {
  local expression="$1"
  cat \
    "${SCIE_JUMP}" \
    "${JDK}" \
    cowsay-1.1.0.jar \
    <(echo) \
    <(
      jq -c "
      setpath([\"scie\", \"jump\", \"size\"]; ${SCIE_JUMP_SIZE})
      | setpath([\"scie\", \"jump\", \"version\"]; \"${SCIE_JUMP_VERSION}\")
      | ${expression}
      " "${LIFT}"
    ) > "${SCIE_CAT}"
  chmod +x "${SCIE_CAT}"
}

scie_cat "."

sha256 "${SCIE_CAT}" > "${SCIE_CAT}.sha256"
gc "${PWD}/${SCIE_CAT}.sha256"

./"${SCIE_CAT}" "scie cat"

if [[ "$(sed "s|${JAVA}||" "${JAVA}.sha256")" == "$(sed "s|${SCIE_CAT}||" "${SCIE_CAT}.sha256")" ]];
then
  log "Hashes of ${JAVA} and ${SCIE_CAT} match."
else
  die "Hashes of ${JAVA} and ${SCIE_CAT} were not the same."
fi

scie_cat "del(.scie.jump)"
{
  ./"${SCIE_CAT}" "missing jump" && die "Expected a missing scie.jump to cause a boot error."
} || log "^- Expected boot error observed"

scie_cat "del(.scie.lift.files[0].type)"
{
  ./"${SCIE_CAT}" "missing jump" && die "Expected a missing file type to cause a boot error."
} || log "^- Expected boot error observed"

scie_cat "del(.scie.lift.files[0].hash)"
{
  ./"${SCIE_CAT}" "missing jump" && die "Expected a missing file hash to cause a boot error."
} || log "^- Expected boot error observed"