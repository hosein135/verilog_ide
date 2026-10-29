#!/usr/bin/env bash
# =============================================================================
# run.sh — Auto setup-or-run for Verilog IDE (Linux / macOS / WSL)
#
# Host bootstrap (scripts/host-prep.sh): git + curl via the distro package
# manager (static curl if that cannot run), xz for the Nix installer, free-disk
# check, git work-tree repair, then pinned Nix 2.28.5. Rust and the IDE build
# come from devops/flake.nix (nixpkgs 25.05).
#
# sudo ./run.sh continues as the user who invoked sudo. A different Nix version
# is replaced with 2.28.5. On NixOS, /nix is kept and 2.28.5 is installed into
# the user profile.
#
# First run wants about 8 GiB free for the Nix shell. Later runs keep at least
# 2 GiB free. Override with VERILOG_IDE_INSTALL_FREE_BYTES or
# VERILOG_IDE_MIN_FREE_BYTES.
#
# Flow inside nix develop:
#   1) cargo run (debug) or cargo run --release
#      (builds vendored xezim + wellen/Surfer as path/crate deps, same as the rest of the crate)
#
# Usage:
#   ./run.sh                # debug run
#   ./run.sh --release      # release run
#   ./run.sh --build        # cargo build only (debug)
#   ./run.sh --build --release
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
sanitize_shell_file "${SCRIPT_DIR}/scripts/host-prep.sh"
# shellcheck source=scripts/host-prep.sh
. "${SCRIPT_DIR}/scripts/host-prep.sh"
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
NIX_PIN_VERSION="2.28.5"
NIX_PIN_MAJOR=2
NIX_PIN_MINOR=28
NIX_PIN_PATCH=5
NIX_INSTALL_URL="https://releases.nixos.org/nix/nix-${NIX_PIN_VERSION}/install"
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
RUN_SH_ARGS=("$@")

for arg in "$@"; do
    case "$arg" in
        --help|-h)
            cat <<'EOF'
Verilog IDE — setup or run (Linux / macOS / WSL).

  ./run.sh              debug run
  ./run.sh --release    release run
  ./run.sh --build      cargo build only
  ./run.sh --build --release
  ./run.sh --force-setup
  ./run.sh --prep-only  Nix env only, do not launch

Missing git and curl are installed with the distro package manager
(Homebrew on macOS). If curl still cannot be installed, a static binary
is downloaded. Nix 2.28.5 (the NixOS 25.05 package manager) is required.
Any other Nix is replaced. On NixOS, /nix is kept and 2.28.5 is
installed into your user profile.

sudo ./run.sh continues as the user who invoked sudo.

First run wants about 8 GiB free. Later runs keep at least 2 GiB free.
Override with VERILOG_IDE_INSTALL_FREE_BYTES or VERILOG_IDE_MIN_FREE_BYTES.
EOF
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
    local f="$1" sum
    if ! sum="$(host_file_hash "${f}")"; then
        error "Need sha256sum, shasum, or openssl to cache the Nix env."
        return 1
    fi
    printf '%s\n' "${sum}"
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

ensure_bootstrap_tools() {
    local had_git=0 had_curl=0
    command -v git >/dev/null 2>&1 && had_git=1
    command -v curl >/dev/null 2>&1 && had_curl=1
    if [ "${had_git}" -eq 0 ] || [ "${had_curl}" -eq 0 ]; then
        host_ensure_git_and_curl \
            || warn "Could not install missing git/curl with the system package manager."
        if ! command -v curl >/dev/null 2>&1; then
            ensure_curl
        elif [ "${had_curl}" -eq 0 ]; then
            info "curl: $(curl --version 2>/dev/null | head -1)"
        fi
        if ! command -v git >/dev/null 2>&1; then
            if [ -e "${SCRIPT_DIR}/.git" ]; then
                error "This checkout is a Git work tree, but git is not installed."
                error "Install git and run ./run.sh again."
                exit 1
            fi
            warn "git is not installed. Nix can still download flake inputs; local Git checks are skipped."
        elif [ "${had_git}" -eq 0 ]; then
            info "git: $(git --version 2>/dev/null | head -1)"
        fi
    fi
    ensure_xz || exit 1
}

# The Nix installer refuses to unpack its .tar.xz unless `xz` is on PATH.
# tar -xJf is what actually extracts; this command only has to exist and
# decompress stdin when tar shells out to it.
install_python_xz() {
    command -v python3 >/dev/null 2>&1 || return 1
    python3 -c 'import lzma' >/dev/null 2>&1 || return 1
    mkdir -p "${BOOTSTRAP_BIN}"
    cat > "${BOOTSTRAP_BIN}/xz" <<'EOF'
#!/bin/sh
exec python3 -c 'import lzma, sys
args = [a for a in sys.argv[1:] if a and not a.startswith("-")]
src = open(args[-1], "rb") if args else sys.stdin.buffer
dec = lzma.LZMADecompressor()
while True:
    chunk = src.read(1024 * 1024)
    if not chunk:
        break
    sys.stdout.buffer.write(dec.decompress(chunk))
tail = dec.flush()
if tail:
    sys.stdout.buffer.write(tail)
'
EOF
    chmod 755 "${BOOTSTRAP_BIN}/xz"
    prepend_bootstrap_path
}

ensure_xz() {
    if command -v xz >/dev/null 2>&1; then
        return 0
    fi
    host_ensure_commands xz \
        || warn "Could not install xz with the system package manager."
    if command -v xz >/dev/null 2>&1; then
        info "xz: $(xz --version 2>/dev/null | head -1)"
        return 0
    fi
    step "xz not found — using Python's lzma so the Nix installer can unpack."
    if ! install_python_xz; then
        error "The Nix installer needs xz to unpack its tarball."
        error "Install it and re-run. On Debian/Ubuntu: sudo apt-get install -y xz-utils"
        return 1
    fi
    info "xz: python lzma helper at ${BOOTSTRAP_BIN}/xz"
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

# Root or a usable sudo (daemon install needs elevation on Linux).
can_install_nix_daemon() {
    if [ "$(id -u)" -eq 0 ]; then
        return 0
    fi
    command -v sudo >/dev/null 2>&1
}

# Pick multi-user (--daemon) vs single-user (--no-daemon) before downloading the installer.
# Equivalent manual commands:
#   curl --proto '=https' --tlsv1.2 -L https://nixos.org/nix/install | sh -s -- --daemon
#   curl --proto '=https' --tlsv1.2 -L https://nixos.org/nix/install | sh -s -- --no-daemon
detect_nix_install_mode() {
    if is_macos; then
        echo "daemon"
        return
    fi
    if is_wsl; then
        # The multi-user daemon installer starts systemd units and user
        # namespaces. On WSL that can kill the distro. Always single-user.
        echo "no-daemon"
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
                info "Nix install: single-user (--no-daemon) — WSL does not use the daemon installer"
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

nix_install_flags() {
    local mode
    mode="$(detect_nix_install_mode)"
    case "${mode}" in
        daemon) echo "--daemon --yes" ;;
        *) echo "--no-daemon --yes" ;;
    esac
}

run_with_pty() {
    # Nix's installer (and nix-env) call tcgetattr; without a TTY they abort:
    # "Inappropriate ioctl for device" / Assertion 'pid != -1'.
    # A synthetic PTY (python pty, script) can crash WSL during that ioctl.
    if is_wsl; then
        if [ -t 0 ] && [ -t 1 ]; then
            "$@"
            return $?
        fi
        if [ -r /dev/tty ] && [ -w /dev/tty ]; then
            "$@" < /dev/tty > /dev/tty 2>&1
            return $?
        fi
        error "Nix install on WSL needs an interactive terminal."
        error "Run ./dev_run.sh from a WSL shell, not a background job."
        return 1
    fi
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

nix_activate_bin() {
    local bin="$1" dir
    NIX_BIN="${bin}"
    export NIX_BIN
    dir="$(dirname "${bin}")"
    case ":${PATH}:" in
        *":${dir}:"*) ;;
        *) export PATH="${dir}:${PATH}" ;;
    esac
    hash -r 2>/dev/null || true
}

# Drop a previous Nix (any version) so the pinned installer can run.
# NixOS keeps /nix; that store belongs to the system generation.
nix_remove_previous() {
    local u f
    if host_is_nixos; then
        error "This is NixOS. Refusing to delete /nix."
        error "Upgrade Nix with the system: sudo nixos-rebuild switch"
        return 1
    fi
    info "Removing the previous Nix installation."
    if command -v systemctl >/dev/null 2>&1; then
        host_as_root systemctl stop nix-daemon.socket nix-daemon.service >/dev/null 2>&1 || true
        host_as_root systemctl disable nix-daemon.socket nix-daemon.service >/dev/null 2>&1 || true
        host_as_root systemctl daemon-reload >/dev/null 2>&1 || true
    fi
    for f in /etc/bashrc /etc/bash.bashrc /etc/profile /etc/zsh/zshrc /etc/zshrc; do
        if [ -f "${f}.backup-before-nix" ]; then
            host_as_root cp -a "${f}.backup-before-nix" "${f}" || true
        fi
    done
    host_as_root rm -rf \
        /nix \
        /etc/nix \
        /etc/profile.d/nix.sh \
        /etc/profile.d/nix-daemon.sh \
        /etc/tmpfiles.d/nix-daemon.conf \
        /usr/lib/systemd/system/nix-daemon.service \
        /usr/lib/systemd/system/nix-daemon.socket \
        /etc/systemd/system/nix-daemon.service \
        /etc/systemd/system/nix-daemon.socket \
        /root/.nix-profile /root/.nix-defexpr /root/.nix-channels \
        /root/.local/state/nix /root/.cache/nix \
        || return 1
    rm -rf \
        "${HOME:-}/.nix-profile" "${HOME:-}/.nix-defexpr" "${HOME:-}/.nix-channels" \
        "${HOME:-}/.local/state/nix" "${HOME:-}/.cache/nix" \
        "${HOME:-}/.config/nix"
    if command -v getent >/dev/null 2>&1 && getent group nixbld >/dev/null 2>&1; then
        while IFS= read -r u; do
            [ -n "${u}" ] || continue
            host_as_root userdel "${u}" >/dev/null 2>&1 || true
        done < <(getent passwd | awk -F: '$1 ~ /^nixbld[0-9]+$/ { print $1 }')
        host_as_root groupdel nixbld >/dev/null 2>&1 || true
    fi
    if command -v getent >/dev/null 2>&1 && getent group nix-daemon >/dev/null 2>&1; then
        host_as_root groupdel nix-daemon >/dev/null 2>&1 || true
    fi
    hash -r 2>/dev/null || true
    unset NIX_BIN || true
}

nix_reload_bin() {
    local best
    hash -r 2>/dev/null || true
    source_nix_profile || true
    enable_flakes
    if ! best="$(host_nix_select_bin)"; then
        error "Nix install finished, but the nix command is still missing."
        return 1
    fi
    nix_activate_bin "${best}"
}

nix_version_is_pinned() {
    [ "$1" -eq "${NIX_PIN_MAJOR}" ] \
        && [ "$2" -eq "${NIX_PIN_MINOR}" ] \
        && [ "$3" -eq "${NIX_PIN_PATCH}" ]
}

# Use an already-installed Nix 2.28.5 even when a newer distro or NixOS
# binary is earlier on PATH.
nix_prefer_pinned_bin() {
    local cand maj min pat
    for cand in \
        "${HOME:-}/.nix-profile/bin/nix" \
        "/nix/var/nix/profiles/per-user/${USER:-}/profile/bin/nix" \
        /nix/var/nix/profiles/default/bin/nix \
        /run/current-system/sw/bin/nix
    do
        [ -n "${cand}" ] && [ -x "${cand}" ] || continue
        if read -r maj min pat < <(host_nix_parse_version "${cand}"); then
            if nix_version_is_pinned "${maj}" "${min}" "${pat}"; then
                nix_activate_bin "${cand}"
                return 0
            fi
        fi
    done
    return 1
}

nixpkgs_25_05_rev() {
    local lock="${FLAKE_DIR}/flake.lock" rev
    [ -f "${lock}" ] || return 1
    rev="$(awk '
        /"rev":/ { gsub(/[",]/, "", $2); rev = $2 }
        /"ref": "nixos-25.05"/ { print rev; exit }
    ' "${lock}")"
    [ -n "${rev}" ] || return 1
    printf '%s\n' "${rev}"
}

# NixOS owns /nix. Put the pinned package manager in the user profile.
nix_install_pinned_on_nixos() {
    local rev
    rev="$(nixpkgs_25_05_rev)" || {
        error "Could not read the nixos-25.05 revision from ${FLAKE_DIR}/flake.lock."
        return 1
    }
    info "This is NixOS. Installing Nix ${NIX_PIN_VERSION} into the user profile."
    info "The system /nix store is left in place."
    "${NIX_BIN}" --extra-experimental-features "nix-command flakes" profile install \
        --accept-flake-config \
        "github:NixOS/nixpkgs/${rev}#nixVersions.nix_2_28" || return 1
    if ! nix_prefer_pinned_bin; then
        error "Nix ${NIX_PIN_VERSION} installed, but it is not available in the user profile."
        return 1
    fi
}

nix_install_pinned() {
    local installer flags mode
    if host_is_nixos; then
        error "Nix is not available on this NixOS system."
        error "Boot a generation that includes nix, then run ./run.sh again."
        return 1
    fi
    ensure_curl
    ensure_xz || return 1
    if [ "${NIX_REPLACE:-}" = "1" ]; then
        nix_remove_previous || return 1
    elif ! nix_present && { [ -e /nix ] || [ -e "${HOME}/.nix-profile" ]; }; then
        nix_remove_previous || return 1
    fi

    export USER="${USER:-$(id -un 2>/dev/null || echo root)}"
    local tmp="${TMPDIR:-/tmp}"
    case "${tmp}" in
        */) ;;
        *) tmp="${tmp}/" ;;
    esac
    export TMPDIR="${tmp}"
    export NIX_INSTALLER_YES=1
    export NIX_INSTALLER_NO_CHANNEL_ADD=1

    mkdir -p "${BOOTSTRAP_DIR}/tmp"
    installer="${BOOTSTRAP_DIR}/tmp/nix-installer.sh"
    http_get "${NIX_INSTALL_URL}" "${installer}" || return 1
    chmod +x "${installer}"

    mode="$(detect_nix_install_mode)"
    case "${mode}" in
        daemon) flags="--daemon --yes --no-channel-add" ;;
        *) flags="--no-daemon --yes --no-channel-add" ;;
    esac
    explain_nix_install_choice "${mode}"
    info "Nix installer flags: ${flags}"
    if [ "${mode}" != "daemon" ]; then
        host_ensure_user_nix_dir || return 1
    fi
    # shellcheck disable=SC2086
    if ! run_with_pty sh "${installer}" ${flags}; then
        if is_wsl; then
            error "Nix installer failed. Not running it again on WSL."
            error "If /nix was left behind and you own it: rm -rf /nix ~/.nix-profile"
            return 1
        fi
        if [ "${mode}" = "daemon" ]; then
            warn "Daemon install failed — retrying single-user (--no-daemon)"
            nix_remove_previous || return 1
            host_ensure_user_nix_dir || return 1
            if ! run_with_pty sh "${installer}" --no-daemon --yes --no-channel-add; then
                error "Nix installer failed. If /nix was left behind, remove it and re-run."
                return 1
            fi
        else
            warn "Nix installer failed. Removing the previous tree and retrying once."
            nix_remove_previous || return 1
            host_ensure_user_nix_dir || return 1
            # shellcheck disable=SC2086
            if ! run_with_pty sh "${installer}" ${flags}; then
                error "Nix installer failed. If /nix was left behind, remove it and re-run."
                return 1
            fi
        fi
    fi
    rm -f "${installer}"
    hash -r 2>/dev/null || true
    source_nix_profile || true
    if ! nix_present && ! host_nix_select_bin >/dev/null 2>&1; then
        error "Nix installed but not on PATH. Open a new terminal and re-run."
        return 1
    fi
    nix_reload_bin || return 1
    info "Nix installed: $("${NIX_BIN}" --version 2>/dev/null || true)"
}

nix_replace_and_recheck() {
    if host_is_nixos; then
        error "This is NixOS. Refusing to delete /nix."
        error "Set nix.package = pkgs.nixVersions.nix_2_28 and run: sudo nixos-rebuild switch"
        return 1
    fi
    if [ "${NIX_REPAIR:-}" = "1" ]; then
        error "Nix is still not usable after replacing the previous install."
        return 1
    fi
    export NIX_REPAIR=1
    export NIX_REPLACE=1
    step "Replacing this Nix with pinned Nix ${NIX_PIN_VERSION}."
    nix_install_pinned || return 1
    nix_reload_bin || return 1
    nix_check_existing
}

# Start a stopped daemon, or re-enter as a member of nix-daemon.
# Returns 0 when the caller should run the Nix check again.
nix_revive_store() {
    local msg="$1" members a quoted
    case "${msg}" in
        *daemon*|*socket*|*Connection\ refused*|*disconnected*)
            if command -v systemctl >/dev/null 2>&1; then
                info "Starting the Nix daemon from the previous install."
                host_as_root systemctl start nix-daemon.socket nix-daemon.service >/dev/null 2>&1 || true
                return 0
            fi
            ;;
    esac
    case "${msg}" in
        *Permission\ denied*|*Operation\ not\ permitted*|*not\ allowed*)
            if [ "${NIX_SG:-}" = "1" ]; then
                return 1
            fi
            if command -v getent >/dev/null 2>&1 && getent group nix-daemon >/dev/null 2>&1; then
                members="$(getent group nix-daemon | awk -F: '{ print $4 }')"
                case ",${members}," in
                    *",${USER},"*) ;;
                    *)
                        info "Adding ${USER} to the nix-daemon group."
                        host_as_root usermod -aG nix-daemon "${USER}" || return 1
                        ;;
                esac
                if command -v sg >/dev/null 2>&1; then
                    info "Opening the Nix store as a member of nix-daemon."
                    export NIX_SG=1
                    quoted=""
                    if [ "${#RUN_SH_ARGS[@]}" -gt 0 ]; then
                        for a in "${RUN_SH_ARGS[@]}"; do
                            quoted="${quoted} $(printf '%q' "${a}")"
                        done
                    fi
                    exec sg nix-daemon -c "cd $(printf '%q' "${SCRIPT_DIR}") && exec $(printf '%q' "${SCRIPT_DIR}/run.sh")${quoted}"
                fi
            fi
            ;;
    esac
    return 1
}

nix_foreign_channel() {
    local url="" ver="" f
    if command -v nix-channel >/dev/null 2>&1; then
        url="$(nix-channel --list 2>/dev/null | awk '$1=="nixpkgs" || $1=="nixos" { print $2; exit }' || true)"
    fi
    for f in \
        "${HOME:-}/.nix-defexpr/channels/nixpkgs/.version" \
        "/nix/var/nix/profiles/per-user/${USER:-}/channels/nixpkgs/.version" \
        "/nix/var/nix/profiles/per-user/root/channels/nixpkgs/.version"
    do
        [ -f "${f}" ] || continue
        ver="$(tr -d '[:space:]' < "${f}" || true)"
        [ -n "${ver}" ] && break
    done
    if [ -z "${url}" ] && [ -z "${ver}" ]; then
        return 0
    fi
    if [[ "${url}" =~ nixos-25\.05([^0-9]|$) ]]; then
        return 0
    fi
    if [ -z "${url}" ] && [[ "${ver}" =~ ^25\.05([^0-9]|$) ]]; then
        return 0
    fi
    if [ -n "${url}" ] && [ -n "${ver}" ]; then
        printf '%s (checkout %s)\n' "${url}" "${ver}"
    elif [ -n "${url}" ]; then
        printf '%s\n' "${url}"
    else
        printf 'checkout %s\n' "${ver}"
    fi
}

nix_store_error() {
    local msg="$1" line
    error "Nix is installed, but the existing store cannot be used."
    while IFS= read -r line; do
        [ -n "${line}" ] || continue
        error "${line}"
    done <<EOF
$(printf '%s\n' "${msg}" | head -n 6)
EOF
    case "${msg}" in
        *experimental*feature*|*flakes*)
            error "This run sets experimental-features in NIX_CONFIG. If that is overridden, add to nix.conf:"
            error "  experimental-features = nix-command flakes"
            ;;
        *daemon*|*socket*|*Connection\ refused*|*disconnected*)
            error "Start the Nix daemon:  sudo systemctl start nix-daemon"
            error "If this user was just added to the nix-daemon group, log out and back in."
            ;;
        *Permission\ denied*|*Operation\ not\ permitted*|*not\ allowed*)
            error "This user cannot access the store from the previous Nix install."
            error "Multi-user Nix: add the user to nix-daemon and log in again."
            error "Single-user Nix: run as the user who originally installed Nix."
            ;;
        *database*|*schema*|*not\ compatible*)
            error "The Nix database was written by a different Nix version."
            error "Re-run ./run.sh to replace it with Nix ${NIX_PIN_VERSION}."
            ;;
        *)
            error "Re-run ./run.sh after the previous Nix is removed, or reinstall from ${NIX_INSTALL_URL}"
            ;;
    esac
}

nix_expected_system() {
    local os arch cpu sys
    os="$(uname -s)"
    arch="$(uname -m)"
    case "${os}" in
        Linux) sys="linux" ;;
        Darwin) sys="darwin" ;;
        *) return 1 ;;
    esac
    case "${arch}" in
        x86_64|amd64) cpu="x86_64" ;;
        aarch64|arm64) cpu="aarch64" ;;
        *) return 1 ;;
    esac
    printf '%s-%s\n' "${cpu}" "${sys}"
}

# Previous Nix: too old for flake.lock, a different store schema, or a
# daemon left behind by an earlier install. A newer working Nix is kept.
nix_check_existing() {
    local first best maj min pat channel err json url sver
    local smaj smin sys feats store_msg expected

    source_nix_profile || true
    enable_flakes
    first="$(command -v nix 2>/dev/null || true)"
    if nix_prefer_pinned_bin; then
        best="${NIX_BIN}"
    elif ! best="$(host_nix_select_bin)"; then
        if [ "${NIX_REPAIR:-}" = "1" ]; then
            error "Nix binaries still do not run after reinstall."
            return 1
        fi
        if host_is_nixos; then
            error "Nix is not available on this NixOS system."
            error "Boot a generation that includes nix, then run ./run.sh again."
            return 1
        fi
        info "The Nix binary on this machine does not run. Replacing it."
        nix_replace_and_recheck || return 1
        return 0
    else
        nix_activate_bin "${best}"
        if [ -n "${first}" ] && [ "${first}" != "${best}" ]; then
            info "Using ${best} (newer Nix than ${first})."
        fi
    fi

    if ! read -r maj min pat < <(host_nix_parse_version "${NIX_BIN}"); then
        if [ "${NIX_REPAIR:-}" = "1" ]; then
            error "Nix at ${NIX_BIN} still does not report a version."
            return 1
        fi
        info "Nix at ${NIX_BIN} did not report a version. Replacing it."
        nix_replace_and_recheck || return 1
        return 0
    fi

    channel="$(nix_foreign_channel || true)"
    if ! nix_version_is_pinned "${maj}" "${min}" "${pat}"; then
        if [ "${NIX_REPAIR:-}" = "1" ]; then
            error "Nix ${maj}.${min}.${pat} is still not Nix ${NIX_PIN_VERSION} after reinstall."
            return 1
        fi
        if host_is_nixos; then
            if [ "${NIX_NIXOS_PIN_TRIED:-}" = "1" ]; then
                error "This NixOS system still provides Nix ${maj}.${min}.${pat}, not Nix ${NIX_PIN_VERSION}."
                error "Set nix.package = pkgs.nixVersions.nix_2_28 and run: sudo nixos-rebuild switch"
                return 1
            fi
            export NIX_NIXOS_PIN_TRIED=1
            nix_install_pinned_on_nixos || return 1
            nix_check_existing || return 1
            return 0
        fi
        info "Nix ${maj}.${min}.${pat} is not pinned Nix ${NIX_PIN_VERSION} (NixOS 25.05). Replacing it."
        if [ -n "${channel}" ]; then
            info "Existing nixpkgs channel: ${channel}"
        fi
        nix_replace_and_recheck || return 1
        return 0
    fi

    if [ -n "${channel}" ]; then
        warn "A previous nixpkgs channel is ${channel}."
        warn "That channel is outside NixOS 25.05. This run ignores it."
        warn "Packages come from devops/flake.lock (github:NixOS/nixpkgs/nixos-25.05)."
    fi

    err="${TMPDIR:-/tmp}/verilog-ide-nix-check.$$"
    json="$("${NIX_BIN}" store info --json 2>"${err}" || true)"
    if [ -z "${json}" ]; then
        json="$("${NIX_BIN}" store ping --json 2>"${err}" || true)"
    fi
    if [ -z "${json}" ]; then
        store_msg="$(cat "${err}" 2>/dev/null || true)"
        rm -f "${err}"
        if [ "${NIX_STORE_RETRY:-}" != "1" ]; then
            export NIX_STORE_RETRY=1
            if nix_revive_store "${store_msg}"; then
                nix_check_existing || return 1
                return 0
            fi
        fi
        if [ "${NIX_REPAIR:-}" != "1" ]; then
            info "The existing Nix store is unusable. Installing Nix ${NIX_PIN_VERSION}."
            nix_replace_and_recheck || return 1
            return 0
        fi
        nix_store_error "${store_msg}"
        return 1
    fi
    rm -f "${err}"

    url="$(printf '%s' "${json}" | sed -n 's/.*"url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    sver="$(printf '%s' "${json}" | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    if [[ "${sver}" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
        smaj="${BASH_REMATCH[1]}"
        smin="${BASH_REMATCH[2]}"
        if [ "${smaj}" != "${maj}" ] || [ "${smin}" != "${min}" ]; then
            if [ "${NIX_REPAIR:-}" = "1" ]; then
                error "Nix client ${maj}.${min}.${pat} still does not match the store (${sver})."
                return 1
            fi
            info "Nix client ${maj}.${min}.${pat} does not match the store (${sver}). Replacing Nix."
            nix_replace_and_recheck || return 1
            return 0
        fi
    fi

    if [ -S /nix/var/nix/daemon-socket/socket ] && [[ "${url}" != daemon* ]]; then
        warn "A Nix daemon socket exists, but ${NIX_BIN} is using a local store."
        warn "A single-user Nix and a multi-user Nix are both installed."
    fi

    if [[ "${url}" == daemon* ]] && ! grep -q '^nixbld1:' /etc/passwd 2>/dev/null; then
        if [ "${NIX_REPAIR:-}" = "1" ]; then
            error "The Nix daemon still has no nixbld build users."
            return 1
        fi
        info "The Nix daemon has no nixbld build users. Replacing Nix."
        nix_replace_and_recheck || return 1
        return 0
    fi

    if ! "${NIX_BIN}" flake --help >/dev/null 2>&1; then
        if [ "${NIX_REPAIR:-}" = "1" ]; then
            error "Nix ${maj}.${min}.${pat} still cannot run flakes."
            return 1
        fi
        info "Nix ${maj}.${min}.${pat} cannot run flakes. Replacing it."
        nix_replace_and_recheck || return 1
        return 0
    fi

    sys="$("${NIX_BIN}" config show system 2>/dev/null || true)"
    expected="$(nix_expected_system 2>/dev/null || true)"
    if [ -n "${sys}" ] && [ -n "${expected}" ] && [ "${sys}" != "${expected}" ]; then
        error "Nix is configured for system ${sys}. This machine is ${expected}."
        error "Fix the system setting in /etc/nix/nix.conf or ~/.config/nix/nix.conf."
        return 1
    fi

    if [ "$("${NIX_BIN}" config show restrict-eval 2>/dev/null || true)" = "true" ]; then
        error "Nix restrict-eval is enabled, so this flake cannot fetch NixOS 25.05."
        error "Set restrict-eval = false in the Nix configuration and retry."
        return 1
    fi

    feats="$("${NIX_BIN}" config show experimental-features 2>/dev/null || true)"
    if ! printf '%s' "${feats}" | grep -q 'flakes' \
        || ! printf '%s' "${feats}" | grep -q 'nix-command'; then
        warn "nix.conf does not list flakes. This run enables them with NIX_CONFIG."
    fi

    info "Nix ${maj}.${min}.${pat} (pinned for NixOS 25.05)"
    return 0
}

ensure_nix() {
    source_nix_profile || true
    enable_flakes
    if nix_present || host_nix_select_bin >/dev/null 2>&1; then
        nix_check_existing || exit 1
        return 0
    fi
    step "Nix not found — installing Nix ${NIX_PIN_VERSION} (no OS package manager) ..."
    nix_install_pinned || exit 1
    nix_check_existing || exit 1
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

    host_rerun_as_desktop_user "${SCRIPT_DIR}/run.sh" "$@" || exit 1
    check_host_os || exit 1
    init_system_cache_paths
    ensure_bootstrap_tools
    host_prepare_git "${SCRIPT_DIR}" true || exit 1
    if [ "${FORCE_SETUP}" = true ]; then
        invalidate_system_nix_cache
    fi
    host_check_free_space || exit 1
    ensure_nix || exit 1

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
