#!/usr/bin/env bash
# =============================================================================
# run.sh — Auto setup-or-run for Verilog IDE (Linux / macOS / WSL)
#
# Host bootstrap (no OS package manager): curl (static binary if missing) + Nix
# (official installer). Rust, iced build deps, and the rest come from flake.nix
# (nixpkgs 25.05). Never apt/dnf/pacman/brew. Never Docker.
#
# Package policy: besides curl + Nix, every tool must be a flake/Nix package.
#
# First run on a machine: ensure curl + Nix, lock flake, download the shell into
# a system cache (~/.cache/verilog-ide/<flake-hash>/). That fetch happens once.
# Later runs: reuse the cached lock + profile + print-dev-env (no GitHub / nixpkgs
# re-download, no nix flake metadata). A new clone on the same system reuses it.
#
# Flow inside nix develop:
#   1) cargo run (debug) or cargo run --release
#      (builds vendored xezim + wellen/Surfer as path/crate deps, same as the rest of the crate)
#
# Usage:
#   ./run.sh                # debug run
#   ./run.sh --release      # release run
#   ./run.sh --build        # cargo build only (debug)
#   ./run.sh --force-setup  # re-fetch Nix packages into the system cache
#   ./run.sh --prep-only    # ensure Nix env only, do not launch
#   ./run.sh --help
# =============================================================================
set -euo pipefail

RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${GREEN}[verilog-ide]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[verilog-ide]${NC}  $*"; }
error() { echo -e "${RED}[verilog-ide]${NC} $*" >&2; }
step()  { echo -e "${CYAN}[verilog-ide]${NC}  $*"; }

sanitize_shell_file() {
    local f="$1"
    [ -f "${f}" ] || return 0
    if ! grep -q $'\r' "${f}" 2>/dev/null; then
        return 0
    fi
    warn "Fixing Windows (CRLF) line endings in $(basename "${f}") ..."
    local tmp="${f}.lf.$$"
    tr -d '\r' < "${f}" > "${tmp}" && mv -f "${tmp}" "${f}"
}

sanitize_shell_file "${BASH_SOURCE[0]}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"
READY_MARKER="${SCRIPT_DIR}/.verilog-ide-nix-ready"
BOOTSTRAP_DIR="${SCRIPT_DIR}/.verilog-ide-bootstrap"
BOOTSTRAP_BIN="${BOOTSTRAP_DIR}/bin"
FLAKE_DIR="${SCRIPT_DIR}/devops"
# Nix 2.25+ defaults to --no-update-lock-file. A flake under a git repo is
# rewritten to git+file://…?dir=devops, which ignores untracked files and then
# demands a lock update. Stage flake.nix + flake.lock outside the git tree
# (~/.cache/verilog-ide/…/flake) and always use that path: URI.
SYSTEM_FLAKE=""
flake_ref() { echo "path:${SYSTEM_FLAKE}"; }
cache_lock_frozen() {
    [ "${FORCE_SETUP}" != true ] \
        && [ -f "${SYSTEM_READY}" ] \
        && [ -e "${SYSTEM_PROFILE}" ] \
        && [ -f "${SYSTEM_DEVENV}" ] \
        && [ -f "${FLAKE_DIR}/flake.lock" ]
}
CURL_STATIC_VERSION="8.20.0"
NIX_INSTALL_URL="https://releases.nixos.org/nix/nix-2.24.12/install"
SYSTEM_CACHE_ROOT="${XDG_CACHE_HOME:-${HOME}/.cache}/verilog-ide"
SYSTEM_CACHE=""
SYSTEM_PROFILE=""
SYSTEM_DEVENV=""
SYSTEM_READY=""
SYSTEM_LOCK=""

FORCE_SETUP=false
PREP_ONLY=false
RELEASE=false
BUILD_ONLY=false
DO_LAUNCH=false

for arg in "$@"; do
    case "$arg" in
        --help|-h)
            sed -n '3,26p' "$0" | sed 's/^# //'
            exit 0 ;;
        --__launch)    DO_LAUNCH=true ;;
        --force-setup) FORCE_SETUP=true ;;
        --prep-only)   PREP_ONLY=true ;;
        --release)     RELEASE=true ;;
        --build)       BUILD_ONLY=true ;;
        *)
            warn "Unknown argument: $arg" ;;
    esac
done

source_nix_profile() {
    if [ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]; then
        # shellcheck source=/dev/null
        . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
    elif [ -f "${HOME}/.nix-profile/etc/profile.d/nix.sh" ]; then
        # shellcheck source=/dev/null
        . "${HOME}/.nix-profile/etc/profile.d/nix.sh"
    elif [ -f /etc/profile.d/nix.sh ]; then
        # shellcheck source=/dev/null
        . /etc/profile.d/nix.sh
    fi
}

enable_flakes() {
    export NIX_CONFIG="${NIX_CONFIG:-}
experimental-features = nix-command flakes
tarball-ttl = 31536000
warn-dirty = false
"
}

file_hash() {
    local f="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "${f}" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "${f}" | awk '{print $1}'
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "${f}" | awk '{print $NF}'
    else
        error "Need sha256sum, shasum, or openssl to cache the Nix env."
        return 1
    fi
}

init_system_cache_paths() {
    local key
    if [ ! -f "${FLAKE_DIR}/flake.nix" ]; then
        error "flake.nix missing in ${FLAKE_DIR}"
        exit 1
    fi
    key="$(file_hash "${FLAKE_DIR}/flake.nix")"
    SYSTEM_CACHE="${SYSTEM_CACHE_ROOT}/${key}"
    SYSTEM_PROFILE="${SYSTEM_CACHE}/profile"
    SYSTEM_DEVENV="${SYSTEM_CACHE}/devenv.sh"
    SYSTEM_READY="${SYSTEM_CACHE}/ready"
    SYSTEM_LOCK="${SYSTEM_CACHE}/flake.lock"
    SYSTEM_FLAKE="${SYSTEM_CACHE}/flake"
}

# Copy flake files out of the git worktree so Nix cannot rewrite the URI to
# git+file://…?dir=devops.
stage_flake() {
    mkdir -p "${SYSTEM_FLAKE}"
    cp -f "${FLAKE_DIR}/flake.nix" "${SYSTEM_FLAKE}/flake.nix"
    if [ -f "${FLAKE_DIR}/flake.lock" ]; then
        cp -f "${FLAKE_DIR}/flake.lock" "${SYSTEM_FLAKE}/flake.lock"
    fi
}

restore_cached_flake_lock() {
    if [ ! -f "${FLAKE_DIR}/flake.lock" ] && [ -f "${SYSTEM_LOCK}" ]; then
        mkdir -p "${FLAKE_DIR}"
        cp -f "${SYSTEM_LOCK}" "${FLAKE_DIR}/flake.lock"
        info "Reusing flake.lock already fetched on this system"
    fi
}

invalidate_system_nix_cache() {
    rm -f "${READY_MARKER}" "${SYSTEM_READY}" "${SYSTEM_DEVENV}"
    if [ -n "${SYSTEM_PROFILE}" ]; then
        rm -f "${SYSTEM_PROFILE}" "${SYSTEM_PROFILE}"-*-link 2>/dev/null || true
        rm -rf "${SYSTEM_PROFILE}" 2>/dev/null || true
    fi
}

prepend_bootstrap_path() {
    if [ -d "${BOOTSTRAP_BIN}" ]; then
        case ":${PATH}:" in
            *":${BOOTSTRAP_BIN}:"*) ;;
            *) export PATH="${BOOTSTRAP_BIN}:${PATH}" ;;
        esac
    fi
}

prefer_unix_path() {
    local cleaned="" part oldifs
    oldifs="${IFS}"
    IFS=':'
    # shellcheck disable=SC2086
    for part in ${PATH}; do
        case "${part}" in
            /mnt/[a-zA-Z]/*) continue ;;
            "") continue ;;
        esac
        if [ -z "${cleaned}" ]; then
            cleaned="${part}"
        else
            cleaned="${cleaned}:${part}"
        fi
    done
    IFS="${oldifs}"
    export PATH="${cleaned}"
    hash -r 2>/dev/null || true
}

nix_env_ready() {
    source_nix_profile || true
    enable_flakes
    prepend_bootstrap_path

    command -v nix >/dev/null 2>&1 || return 1
    command -v curl >/dev/null 2>&1 || return 1
    nix flake --help >/dev/null 2>&1 || return 1
    [ -f "${FLAKE_DIR}/flake.nix" ] || return 1
    [ -n "${SYSTEM_READY}" ] && [ -f "${SYSTEM_READY}" ] || return 1
    [ -e "${SYSTEM_PROFILE}" ] || return 1
    restore_cached_flake_lock
    [ -f "${FLAKE_DIR}/flake.lock" ] || return 1
    return 0
}

http_get() {
    local url="$1" dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --proto '=https' --tlsv1.2 -o "${dest}" "${url}"
    else
        error "Cannot download ${url}: curl is required (bootstrap)."
        return 1
    fi
}

extract_tar_xz() {
    local archive="$1" dest="$2"
    mkdir -p "${dest}"
    if tar -xJf "${archive}" -C "${dest}" 2>/dev/null; then
        return 0
    fi
    error "Cannot extract ${archive}: need tar with xz support (tar -xJf)."
    return 1
}

static_curl_asset() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "${os}" in
        Linux)
            case "${arch}" in
                x86_64|amd64)  echo "curl-linux-x86_64-musl-${CURL_STATIC_VERSION}.tar.xz" ;;
                aarch64|arm64) echo "curl-linux-aarch64-musl-${CURL_STATIC_VERSION}.tar.xz" ;;
                *) error "Unsupported Linux arch for static curl: ${arch}"; return 1 ;;
            esac
            ;;
        Darwin)
            case "${arch}" in
                x86_64)        echo "curl-macos-x86_64-${CURL_STATIC_VERSION}.tar.xz" ;;
                arm64|aarch64) echo "curl-macos-arm64-${CURL_STATIC_VERSION}.tar.xz" ;;
                *) error "Unsupported macOS arch for static curl: ${arch}"; return 1 ;;
            esac
            ;;
        *)
            error "Unsupported OS for static curl: ${os}"
            return 1
            ;;
    esac
}

ensure_curl() {
    prepend_bootstrap_path
    if command -v curl >/dev/null 2>&1; then
        info "curl: $(curl --version 2>/dev/null | head -1)"
        return 0
    fi

    step "curl not found — installing static binary (no OS package manager) ..."
    local asset url archive extract_dir found
    asset="$(static_curl_asset)" || exit 1
    url="https://github.com/stunnel/static-curl/releases/download/${CURL_STATIC_VERSION}/${asset}"
    mkdir -p "${BOOTSTRAP_BIN}" "${BOOTSTRAP_DIR}/tmp"
    archive="${BOOTSTRAP_DIR}/tmp/${asset}"
    extract_dir="${BOOTSTRAP_DIR}/tmp/curl-extract-$$"
    rm -rf "${extract_dir}"
    mkdir -p "${extract_dir}"

    http_get "${url}" "${archive}" || exit 1
    extract_tar_xz "${archive}" "${extract_dir}" || exit 1

    found="$(find "${extract_dir}" -type f -name curl 2>/dev/null | head -1 || true)"
    if [ -z "${found}" ]; then
        error "Static curl archive had no 'curl' binary: ${asset}"
        exit 1
    fi
    cp -f "${found}" "${BOOTSTRAP_BIN}/curl"
    chmod +x "${BOOTSTRAP_BIN}/curl"
    rm -rf "${extract_dir}" "${archive}"
    prepend_bootstrap_path
    info "curl installed (static): $(curl --version 2>/dev/null | head -1)"
}

nix_present() {
    source_nix_profile || true
    if command -v nix >/dev/null 2>&1; then
        return 0
    fi
    if [ -x /nix/var/nix/profiles/default/bin/nix ]; then
        export PATH="/nix/var/nix/profiles/default/bin:${PATH}"
        command -v nix >/dev/null 2>&1 && return 0
    fi
    if [ -x "${HOME}/.nix-profile/bin/nix" ]; then
        export PATH="${HOME}/.nix-profile/bin:${PATH}"
        command -v nix >/dev/null 2>&1 && return 0
    fi
    return 1
}

systemd_running() {
    [ -d /run/systemd/system ] || return 1
    command -v systemctl >/dev/null 2>&1 || return 1
    case "$(systemctl is-system-running 2>/dev/null || true)" in
        running|degraded) return 0 ;;
        *) return 1 ;;
    esac
}

is_wsl() {
    if [ -n "${WSL_DISTRO_NAME:-}" ] || [ -n "${WSL_INTEROP:-}" ]; then
        return 0
    fi
    if [ -r /proc/version ] && grep -qiE '(microsoft|wsl)' /proc/version 2>/dev/null; then
        return 0
    fi
    return 1
}

is_macos() {
    [ "$(uname -s)" = Darwin ]
}

is_linux() {
    [ "$(uname -s)" = Linux ]
}

can_install_nix_daemon() {
    if [ "$(id -u)" -eq 0 ]; then
        return 0
    fi
    command -v sudo >/dev/null 2>&1
}

detect_nix_install_mode() {
    if is_macos; then
        echo "daemon"
        return
    fi
    if is_wsl; then
        if systemd_running && can_install_nix_daemon; then
            echo "daemon"
        else
            echo "no-daemon"
        fi
        return
    fi
    if is_linux; then
        if systemd_running && can_install_nix_daemon; then
            echo "daemon"
        else
            echo "no-daemon"
        fi
        return
    fi
    echo "no-daemon"
}

explain_nix_install_choice() {
    local mode="$1"
    local manual_url="https://nixos.org/nix/install"
    case "${mode}" in
        daemon)
            if is_macos; then
                info "Nix install: multi-user (--daemon) — macOS uses launchd"
            elif is_wsl; then
                info "Nix install: multi-user (--daemon) — WSL with systemd + sudo"
            else
                info "Nix install: multi-user (--daemon) — Linux with systemd + sudo"
            fi
            info "Manual equivalent:"
            info "  curl --proto '=https' --tlsv1.2 -L ${manual_url} | sh -s -- --daemon"
            ;;
        *)
            if is_wsl; then
                info "Nix install: single-user (--no-daemon) — WSL without systemd (or no sudo)"
            elif is_linux; then
                info "Nix install: single-user (--no-daemon) — no systemd or no sudo for multi-user"
            else
                info "Nix install: single-user (--no-daemon) — $(uname -s) without systemd/launchd"
            fi
            info "Manual equivalent:"
            info "  curl --proto '=https' --tlsv1.2 -L ${manual_url} | sh -s -- --no-daemon"
            ;;
    esac
    info "This run uses pinned installer: ${NIX_INSTALL_URL}"
}

run_with_pty() {
    if [ -t 0 ] && [ -t 1 ]; then
        "$@"
        return $?
    fi
    if command -v python3 >/dev/null 2>&1; then
        python3 -c 'import pty,sys; raise SystemExit(pty.spawn(sys.argv[1:]))' "$@"
        return $?
    fi
    if command -v script >/dev/null 2>&1; then
        case "$(uname -s)" in
            Darwin)
                script -q /dev/null "$@"
                return $?
                ;;
            *)
                mkdir -p "${BOOTSTRAP_DIR}/tmp"
                local wrapper st
                wrapper="$(mktemp "${BOOTSTRAP_DIR}/tmp/pty-XXXXXX")"
                {
                    printf '#!/usr/bin/env bash\nset -- '
                    printf '%q ' "$@"
                    printf '\nexec "$@"\n'
                } > "${wrapper}"
                chmod +x "${wrapper}"
                script -q -e -c "${wrapper}" /dev/null
                st=$?
                rm -f "${wrapper}"
                return "${st}"
                ;;
        esac
    fi
    warn "No PTY helper — Nix install may fail without a real terminal"
    "$@"
}

remove_incomplete_nix() {
    if nix_present; then
        return 0
    fi
    if [ ! -e /nix ] && [ ! -e "${HOME}/.nix-profile" ]; then
        return 0
    fi
    warn "Previous Nix install did not finish — clearing the incomplete tree"
    if [ "$(id -u)" -eq 0 ] || { [ -e /nix ] && [ -O /nix ]; } || { [ -e /nix ] && [ -w /nix ]; }; then
        rm -rf /nix "${HOME}/.nix-profile" "${HOME}/.nix-defexpr" "${HOME}/.nix-channels" \
            /etc/nix 2>/dev/null || true
        rm -f /etc/profile.d/nix.sh /etc/profile.d/nix-daemon.sh 2>/dev/null || true
    else
        error "Incomplete Nix files in /nix but this user cannot remove them."
        error "As root run:  rm -rf /nix ~/.nix-profile ~/.nix-defexpr ~/.nix-channels"
        error "Then re-run ./run.sh"
        exit 1
    fi
}

ensure_nix() {
    if nix_present; then
        info "Nix: $(nix --version 2>/dev/null || true)"
        return 0
    fi

    step "Nix not found — installing via official installer (no OS package manager) ..."
    ensure_curl
    remove_incomplete_nix

    export USER="${USER:-$(id -un 2>/dev/null || echo root)}"
    local tmp="${TMPDIR:-/tmp}"
    case "${tmp}" in
        */) ;;
        *) tmp="${tmp}/" ;;
    esac
    export TMPDIR="${tmp}"

    mkdir -p "${BOOTSTRAP_DIR}/tmp"
    local installer flags mode
    installer="${BOOTSTRAP_DIR}/tmp/nix-installer.sh"
    http_get "${NIX_INSTALL_URL}" "${installer}" || exit 1
    chmod +x "${installer}"

    mode="$(detect_nix_install_mode)"
    case "${mode}" in
        daemon) flags="--daemon --yes" ;;
        *) flags="--no-daemon --yes" ;;
    esac
    explain_nix_install_choice "${mode}"
    info "Nix installer flags: ${flags}"
    # shellcheck disable=SC2086
    if ! run_with_pty sh "${installer}" ${flags}; then
        if [ "${mode}" = "daemon" ]; then
            warn "Daemon install failed — retrying single-user (--no-daemon)"
            remove_incomplete_nix
            if ! run_with_pty sh "${installer}" --no-daemon --yes; then
                error "Nix installer failed. If /nix was left behind, remove it and re-run."
                exit 1
            fi
        else
            error "Nix installer failed. If /nix was left behind, remove it and re-run."
            exit 1
        fi
    fi

    hash -r 2>/dev/null || true
    source_nix_profile || true
    if ! nix_present; then
        error "Nix installed but not on PATH. Open a new terminal and re-run."
        exit 1
    fi
    info "Nix installed: $(nix --version)"
}

ensure_flakes() {
    enable_flakes
    if ! nix flake --help >/dev/null 2>&1; then
        error "This Nix build does not support flakes. Upgrade Nix, then re-run."
        exit 1
    fi
}

ensure_flake_lock() {
    if [ ! -f "${FLAKE_DIR}/flake.nix" ]; then
        error "flake.nix missing in ${FLAKE_DIR}"
        exit 1
    fi
    restore_cached_flake_lock
    mkdir -p "${SYSTEM_CACHE}"
    stage_flake
    local lock_needs_update=false
    if [ ! -f "${SYSTEM_FLAKE}/flake.lock" ]; then
        lock_needs_update=true
    elif grep -qE 'rust-overlay' "${FLAKE_DIR}/flake.nix" \
        && ! grep -q '"rust-overlay"' "${FLAKE_DIR}/flake.lock" 2>/dev/null; then
        # flake.nix may declare rust-overlay across multiple lines
        # (rust-overlay = { url = ... }), so do not require "rust-overlay.url".
        lock_needs_update=true
    fi
    if [ "${lock_needs_update}" = true ]; then
        step "Updating flake.lock ..."
        # Allow rewriting the staged lock (path flake); copy result back to repo.
        nix flake lock "$(flake_ref)"
        cp -f "${SYSTEM_FLAKE}/flake.lock" "${FLAKE_DIR}/flake.lock"
    fi
    cp -f "${FLAKE_DIR}/flake.lock" "${SYSTEM_LOCK}"
    cp -f "${FLAKE_DIR}/flake.lock" "${SYSTEM_FLAKE}/flake.lock"
}

check_host_os() {
    case "$(uname -s)" in
        Linux|Darwin) ;;
        *)
            error "Unsupported OS: $(uname -s). Use Linux, macOS, or WSL."
            exit 1
            ;;
    esac
}

realize_nix_shell() {
    mkdir -p "${SYSTEM_CACHE}"
    stage_flake
    step "Fetching Nix packages into the system cache (once per machine / flake) ..."
    # --no-update-lock-file: do not rewrite the staged path flake.lock during develop.
    nix develop "$(flake_ref)" \
        --profile "${SYSTEM_PROFILE}" \
        --no-update-lock-file \
        --command true
    if [ -f "${SYSTEM_FLAKE}/flake.lock" ]; then
        cp -f "${SYSTEM_FLAKE}/flake.lock" "${SYSTEM_LOCK}"
        cp -f "${SYSTEM_FLAKE}/flake.lock" "${FLAKE_DIR}/flake.lock"
    fi

    if nix print-dev-env "$(flake_ref)" --offline --no-update-lock-file \
        | tr -d '\r' > "${SYSTEM_DEVENV}.tmp"; then
        mv -f "${SYSTEM_DEVENV}.tmp" "${SYSTEM_DEVENV}"
        sanitize_shell_file "${SYSTEM_DEVENV}"
    else
        rm -f "${SYSTEM_DEVENV}.tmp"
        warn "nix print-dev-env failed — later runs will use nix develop --offline"
    fi
    date -u +"%Y-%m-%dT%H:%M:%SZ" > "${SYSTEM_READY}"
    date -u +"%Y-%m-%dT%H:%M:%SZ" > "${READY_MARKER}"
    info "Nix packages cached on this system → ${SYSTEM_CACHE}"
}

setup_first_time() {
    step "First-time (or incomplete) setup — preparing Verilog IDE Nix environment ..."
    ensure_curl
    ensure_nix
    ensure_flakes
    ensure_flake_lock
    check_host_os
    realize_nix_shell
}

run_gui() {
    exec "$@"
}

launch_app() {
    prefer_unix_path
    cd "${SCRIPT_DIR}"
    export VERILOG_IDE_SAMPLES_DIR="${SCRIPT_DIR}/samples"

    if [ "${PREP_ONLY}" = true ]; then
        info "Prep-only — Nix env ready (rustc: $(rustc --version 2>/dev/null || echo missing))."
        return 0
    fi

    if [ "${BUILD_ONLY}" = true ]; then
        if [ "${RELEASE}" = true ]; then
            step "Building release ..."
            cargo build --release
        else
            step "Building debug ..."
            cargo build
        fi
        info "Build complete."
        return 0
    fi

    if [ "${RELEASE}" = true ]; then
        step "Running Verilog IDE (release) ..."
        run_gui cargo run --release
    else
        step "Running Verilog IDE (debug) ..."
        run_gui cargo run
    fi
}

run_inside_nix() {
    step "Nix environment ready — starting Verilog IDE ..."
    local launch_pid=0

    forward_launch_signal() {
        if [ "${launch_pid}" -gt 0 ] 2>/dev/null && kill -0 "${launch_pid}" 2>/dev/null; then
            kill -INT "${launch_pid}" 2>/dev/null || kill -TERM "${launch_pid}" 2>/dev/null || true
        fi
    }

    trap forward_launch_signal INT TERM

    if [ -f "${SYSTEM_DEVENV}" ]; then
        info "Using Nix env already fetched on this system (no download)"
        sanitize_shell_file "${SYSTEM_DEVENV}"
        (
            set +u
            # shellcheck disable=SC1090
            . "${SYSTEM_DEVENV}"
            cd "${SCRIPT_DIR}"
            exec bash "${SCRIPT_DIR}/run.sh" --__launch "$@"
        ) &
        launch_pid=$!
    else
        stage_flake
        nix develop "$(flake_ref)" \
            --profile "${SYSTEM_PROFILE}" \
            --offline \
            --no-update-lock-file \
            --command bash "${SCRIPT_DIR}/run.sh" --__launch "$@" &
        launch_pid=$!
    fi

    while kill -0 "${launch_pid}" 2>/dev/null; do
        wait "${launch_pid}" 2>/dev/null || true
    done
    trap - INT TERM
}

main() {
    if [ "${DO_LAUNCH}" = true ]; then
        launch_app
        exit 0
    fi

    init_system_cache_paths

    if [ "${FORCE_SETUP}" = true ]; then
        invalidate_system_nix_cache
        setup_first_time
    elif nix_env_ready; then
        info "Verilog IDE Nix environment already cached on this system — starting."
        source_nix_profile || true
        enable_flakes
        restore_cached_flake_lock
        stage_flake
    else
        setup_first_time
    fi

    if [ "${PREP_ONLY}" = true ]; then
        run_inside_nix --prep-only
        info "Prep-only — done."
        exit 0
    fi

    run_inside_nix "$@"
}

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    main "$@"
fi
