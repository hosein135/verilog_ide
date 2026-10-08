#!/usr/bin/env bash
# Self-fix CRLF before strict mode (common on /mnt/c Windows mounts).
if grep -q $'\r' "${BASH_SOURCE[0]:-$0}" 2>/dev/null; then
    _lf="$(mktemp "${TMPDIR:-/tmp}/fix-lf.XXXXXX")"
    tr -d '\r' < "${BASH_SOURCE[0]:-$0}" > "${_lf}"
    chmod +x "${_lf}"
    exec bash "${_lf}" "$@"
fi

# =============================================================================
# fix-line-endings.sh — Convert CRLF to LF for text files in this project
#
# Matches .gitattributes policy: source, config, docs, and HDL files use LF.
# Skips build artifacts, caches, and known binary extensions.
#
# Usage:
#   ./fix-line-endings.sh           # convert files that contain CRLF
#   ./fix-line-endings.sh --dry-run # list files that would be converted
#   ./fix-line-endings.sh --help
# =============================================================================
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${GREEN}[lf]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[lf]${NC}  $*"; }
error() { echo -e "${RED}[lf]${NC} $*" >&2; }

sanitize_shell_file() {
    local f="$1"
    [ -f "${f}" ] || return 0
    if ! grep -q $'\r' "${f}" 2>/dev/null; then
        return 0
    fi
    local tmp="${f}.lf.$$"
    tr -d '\r' < "${f}" > "${tmp}" && mv -f "${tmp}" "${f}"
}

sanitize_shell_file "${BASH_SOURCE[0]}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

DRY_RUN=false

for arg in "$@"; do
    case "$arg" in
        --help|-h)
            sed -n '11,20p' "$0" | sed 's/^# //'
            exit 0 ;;
        --dry-run) DRY_RUN=true ;;
        *)
            error "Unknown argument: $arg"
            exit 1 ;;
    esac
done

is_binary_ext() {
    case "${1,,}" in
        exe|dll|lib|a|o|obj|pdb|rlib|rmeta|png|jpg|jpeg|gif|ico|wasm|zip|tar|gz|xz|bz2|7z|pdf|woff|woff2|ttf|eot|so|dylib)
            return 0 ;;
        *)
            return 1 ;;
    esac
}

is_text_file() {
    local rel="$1"
    local base ext
    base="$(basename "${rel}")"
    ext="${rel##*.}"
    if [ "${base}" = "${rel}" ] || [ "${ext}" = "${rel}" ]; then
        ext=""
    fi

    case "${base}" in
        .gitignore|.gitattributes|flake.nix|flake.lock|Cargo.lock)
            return 0 ;;
    esac

    case "${ext,,}" in
        rs|toml|md|json|yml|yaml|sh|nix|v|vh|sv|svh|vl|txt|cfg|do|tcl|lock|gitignore|gitattributes)
            return 0 ;;
    esac

    return 1
}

convert_file() {
    local file="$1"
    if ! grep -q $'\r' "${file}" 2>/dev/null; then
        return 1
    fi
    if [ "${DRY_RUN}" = true ]; then
        echo "${file#${SCRIPT_DIR}/}"
        return 0
    fi
    local tmp="${file}.lf.$$"
    tr -d '\r' < "${file}" > "${tmp}" && mv -f "${tmp}" "${file}"
    info "Converted: ${file#${SCRIPT_DIR}/}"
    return 0
}

converted=0

while IFS= read -r -d '' file; do
    rel="${file#${SCRIPT_DIR}/}"

    base="$(basename "${rel}")"
    ext="${rel##*.}"
    if [ "${base}" = "${rel}" ] || [ "${ext}" = "${rel}" ]; then
        ext=""
    fi

    if is_binary_ext "${ext}"; then
        continue
    fi
    if ! is_text_file "${rel}"; then
        continue
    fi

    if convert_file "${file}"; then
        converted=$((converted + 1))
    fi
done < <(
    find "${SCRIPT_DIR}" \
        \( \
            -path "${SCRIPT_DIR}/.git/*" -o \
            -path "${SCRIPT_DIR}/target/*" -o \
            -path "${SCRIPT_DIR}/.vfox/*" -o \
            -path "${SCRIPT_DIR}/node_modules/*" -o \
            -path "${SCRIPT_DIR}/.verilog-ide-bootstrap/*" -o \
            -path "${SCRIPT_DIR}/.verilog-ide-data/*" -o \
            -path "${SCRIPT_DIR}/.idea/*" -o \
            -path "${SCRIPT_DIR}/.vscode/*" \
        \) -prune -o \
        -type f -print0
)

if [ "${DRY_RUN}" = true ]; then
    info "Would convert ${converted} file(s)."
else
    info "Done — converted ${converted} file(s) to LF."
fi
