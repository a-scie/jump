#!/usr/bin/env bash

set -euo pipefail

directory=""
file=""
exists=""
while getopts "d:e:f:" option; do
  case "$option" in
    d) directory="${OPTARG}";;
    f) file="${OPTARG}";;
    e) exists="${OPTARG}";;
    *)
      echo "Usage: $0 [-d <dir>] [-f <file>] [-e <file or dir>]"
      exit 1
      ;;
  esac
done

if [ -n "${directory}" ]; then
  if [ -d "${directory}" ]; then
    echo "Directory:"
    stat "${directory}"
  else
    echo "Directory does not exist: ${directory}" >&2
    ls -la "${directory}" >&2
    exit 1
  fi
fi

if [ -n "${file}" ]; then
  if [ -f "${file}" ]; then
    echo "File:"
    stat "${file}"
  else
    echo "File does not exist: "${file} >&2
    exit 1
  fi
fi

if [ -n "${exists}" ]; then
  if [ -e "${exists}" ]; then
    echo "Exists:"
    stat "${exists}"
  else
    echo "Does not exist: ${exists}" >&2
    exit 1
  fi
fi

echo