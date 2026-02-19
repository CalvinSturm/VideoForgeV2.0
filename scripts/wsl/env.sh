#!/usr/bin/env bash
set -euo pipefail

if grep -qi microsoft /proc/version 2>/dev/null; then
  export LD_LIBRARY_PATH="/usr/lib/wsl/lib:/usr/local/cuda-12/targets/x86_64-linux/lib:${LD_LIBRARY_PATH:-}"
  wsl_link_flag="-Lnative=/usr/lib/wsl/lib"
  case " ${RUSTFLAGS:-} " in
    *" ${wsl_link_flag} "*) ;;
    *) export RUSTFLAGS="${wsl_link_flag}${RUSTFLAGS:+ ${RUSTFLAGS}}" ;;
  esac
fi
