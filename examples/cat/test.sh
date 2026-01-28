# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

check_cmd cat chmod cut dirname jq ln stat

function size() {
  if [[ "${OS}" == "macos" ]]; then
    stat -f %z "$1"
  else
    stat -c %s "$1"
  fi
}

if ! "${SCIE_JUMP}" --launch="${LIFT}" "Unpacked Launch!" | grep "< Unpacked Launch! >"; then
  die "Execution of the unpacked scie failed."
else
  echo "Unpacked execution of ${LIFT} directly with the ${SCIE_JUMP} works."
fi

gc "${PWD}/lift.json"
ln -s "${LIFT}" lift.json
if ! "${SCIE_JUMP}" -x "Unpacked Implicit Manifest Launch!" | grep "< Unpacked Implicit Manifest Launch! >"; then
  die "Execution of the unpacked scie failed."
else
  echo "Unpacked execution of the implicit ${PWD}/lift.json with the ${SCIE_JUMP} works."
fi

(
  abs_lift_path="${PWD}/${LIFT}"
  cd "$(dirname "${SCIE_JUMP}")"
  if ! "${SCIE_JUMP}" --launch="${abs_lift_path}" "Unpacked PWD different Launch!" | grep "< Unpacked PWD different Launch! >"; then
    die "Execution of the unpacked scie failed."
  else
    echo "Unpacked execution of ${abs_lift_path} directly with the ${SCIE_JUMP} works from ${PWD}."
  fi
)

SCIE=help "${SCIE_JUMP}" -x || die "SCIE=help for an unpacked scie failed."
SCIE=list "${SCIE_JUMP}" -x || die "SCIE=list for an unpacked scie failed."
SCIE=inspect "${SCIE_JUMP}" -x || die "SCIE=inspect for an unpacked scie failed."

if [[ "${OS}" == "windows" ]]; then
  install_dir="${PWD}\install"
else
  install_dir="${PWD}/install"
fi
gc "${install_dir}"
mkdir -v "${install_dir}"
SCIE=install "${SCIE_JUMP}" -x "${install_dir}" || die "SCIE=install for an unpacked scie failed."

if [[ "${OS}" == "windows" ]]; then
  shim_script="${install_dir}\java.ps1"
else
  shim_script="${install_dir}/java"
fi
if [[ -f "${shim_script}" ]]; then
  echo "The expected shim script at ${shim_script} was generated with contents:"
  cat "${shim_script}"
else
  die "The expected shim script at ${shim_script} was not generated."
fi

if ! "${shim_script}" "Shim Launch!!" | grep "< Shim Launch!! >"; then
  die "Execution of unpacked scie installed shim script failed."
else
  echo "Execution of unpacked scie shim script at ${install_dir}/java succeeded:"
fi

SCIE_JUMP_ALT="scie-jump-${OS_ARCH}${EXE_EXT}"
chmod +x "${SCIE_JUMP_ALT}"
EXPECTED_SIZE="$(size "${SCIE_JUMP_ALT}")"

JAVA="java${EXE_EXT}"
gc "${PWD}/${JAVA}"
"${SCIE_JUMP}" -sj "${SCIE_JUMP_ALT}" "${LIFT}"

sha256 "${JAVA}" > "${JAVA}.sha256"
gc "${PWD}/${JAVA}.sha256"

./"${JAVA}" "scie-jump boot-pack"
gc "${PWD}/split"
SCIE="split" ./"${JAVA}" split -- scie-jump
SPLIT_SCIE_JUMP="split/scie-jump${EXE_EXT}"

ACTUAL_SIZE="$(size "${SPLIT_SCIE_JUMP}")"
if [[ "${EXPECTED_SIZE}" != "${ACTUAL_SIZE}" ]]; then
  die "The scie-jump in the ${JAVA} scie tip is ${ACTUAL_SIZE} bytes; expected: ${EXPECTED_SIZE} bytes"
else
  echo "Found scie-jump in the ${JAVA} scie tip with the expected size."
fi

ACTUAL_VERSION="$("${SPLIT_SCIE_JUMP}" -V)"
if [[ "1.8.1" != "${ACTUAL_VERSION}" ]]; then
  die "The scie-jump in the ${JAVA} scie tip reports version ${ACTUAL_VERSION}; expected: 1.8.1"
else
  echo "Found scie-jump in the ${JAVA} scie tip with the expected version."
fi

REPORTED_SIZE="$(SCIE="inspect" ./"${JAVA}" | jq -r '.scie.jump.size')"
if [[ "${EXPECTED_SIZE}" != "${REPORTED_SIZE}" ]]; then
  die "SCIE=inspect ./${JAVA} reported scie-jump size ${REPORTED_SIZE}; expected: ${EXPECTED_SIZE}"
else
  echo "SCIE=inspect ./${JAVA} reported the correct scie-jump size."
fi

REPORTED_VERSION="$(SCIE="inspect" ./"${JAVA}" | jq -r '.scie.jump.version')"
if [[ "1.8.1" != "${REPORTED_VERSION}" ]]; then
  die "SCIE=inspect ./${JAVA} reported scie-jump version ${REPORTED_VERSION}; expected: 1.8.1"
else
  echo "SCIE=inspect ./${JAVA} reported the correct scie-jump version."
fi

# We should be able to assemble an identical scie using cat.
JDK="$(jq -r '.scie.lift.files[0].name' "${LIFT}")"
SCIE_CAT="java.cat${EXE_EXT}"
gc "${PWD}/${SCIE_CAT}"

function scie_cat() {
  local expression="$1"
  cat \
    "${SCIE_JUMP_ALT}" \
    "${JDK}" \
    cowsay-1.1.0.jar \
    <(echo -en "${NEWLINE}") \
    <(
      jq -c "
      setpath([\"scie\", \"jump\", \"size\"]; ${ACTUAL_SIZE})
      | setpath([\"scie\", \"jump\", \"version\"]; \"${ACTUAL_VERSION}\")
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
