#!/usr/bin/env sh
# Install envprobe from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/HoshiyomiLusia/envprobe/main/install.sh | sh
#
# Environment variables:
#   ENVPROBE_VERSION     Specific version to install (e.g. v0.1.0). Default: latest.
#   ENVPROBE_INSTALL_DIR Override install directory. Default: /usr/local/bin, else ~/.local/bin.

set -eu

REPO="HoshiyomiLusia/envprobe"
GITHUB_URL="https://github.com/${REPO}"
API_URL="https://api.github.com/repos/${REPO}"

err() {
    printf 'envprobe install: %s\n' "$*" >&2
    exit 1
}

info() {
    printf '%s\n' "$*"
}

require() {
    command -v "$1" >/dev/null 2>&1 || err "required command not found: $1"
}

detect_os() {
    case "$(uname -s)" in
        Darwin) printf macos ;;
        Linux)  printf linux ;;
        *)      err "unsupported OS: $(uname -s). On Windows, use the PowerShell installer: irm https://raw.githubusercontent.com/${REPO}/main/install.ps1 | iex" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  printf x86_64 ;;
        aarch64|arm64) printf aarch64 ;;
        *)             err "unsupported architecture: $(uname -m)" ;;
    esac
}

resolve_version() {
    if [ -n "${ENVPROBE_VERSION:-}" ]; then
        case "$ENVPROBE_VERSION" in
            v*) printf '%s' "$ENVPROBE_VERSION" ;;
            *)  printf 'v%s' "$ENVPROBE_VERSION" ;;
        esac
        return
    fi
    tag=$(curl -fsSL "${API_URL}/releases/latest" \
        | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' \
        | head -1)
    if [ -z "$tag" ]; then
        err "could not resolve latest envprobe version from ${API_URL}/releases/latest"
    fi
    printf '%s' "$tag"
}

resolve_install_dir() {
    if [ -n "${ENVPROBE_INSTALL_DIR:-}" ]; then
        mkdir -p "$ENVPROBE_INSTALL_DIR" 2>/dev/null || true
        printf '%s' "$ENVPROBE_INSTALL_DIR"
        return
    fi
    # Prefer /usr/local/bin if writable (already on PATH); else ~/.local/bin; else sudo.
    if [ -w "/usr/local/bin" ]; then
        printf '%s' "/usr/local/bin"
        return
    fi
    local_bin="${HOME}/.local/bin"
    if mkdir -p "$local_bin" 2>/dev/null && [ -w "$local_bin" ]; then
        printf '%s' "$local_bin"
        return
    fi
    printf '%s' "/usr/local/bin"
}

verify_checksum() {
    archive=$1
    checksum_file=$2
    dir=$(dirname "$archive")
    base=$(basename "$archive")
    base_sum=$(basename "$checksum_file")
    if command -v shasum >/dev/null 2>&1; then
        (cd "$dir" && shasum -a 256 -c "$base_sum")
    elif command -v sha256sum >/dev/null 2>&1; then
        (cd "$dir" && sha256sum -c "$base_sum")
    else
        err "neither shasum nor sha256sum is available"
    fi
}

install_binary() {
    src=$1
    install_dir=$2
    if [ -w "$install_dir" ]; then
        cp "$src" "$install_dir/envprobe"
    elif [ -d "$install_dir" ]; then
        sudo cp "$src" "$install_dir/envprobe"
    else
        sudo mkdir -p "$install_dir"
        sudo cp "$src" "$install_dir/envprobe"
    fi
}

main() {
    require curl
    require tar
    require uname

    os=$(detect_os)
    arch=$(detect_arch)

    if [ "$os" = "macos" ] && [ "$arch" = "x86_64" ]; then
        err "no prebuilt binary for macos x86_64; install with: cargo install --git ${GITHUB_URL}"
    fi

    version=$(resolve_version)
    install_dir=$(resolve_install_dir)

    name="envprobe-${version}-${os}-${arch}"
    asset="${name}.tar.gz"
    asset_url="${GITHUB_URL}/releases/download/${version}/${asset}"
    checksum_url="${asset_url}.sha256"

    tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/envprobe-install.XXXXXX")
    trap 'rm -rf "$tmp_dir"' EXIT INT TERM

    info "Downloading ${asset} ..."
    curl -fsSL "$asset_url" -o "$tmp_dir/$asset" \
        || err "failed to download $asset_url"

    info "Fetching SHA-256 checksum ..."
    curl -fsSL "$checksum_url" -o "$tmp_dir/$asset.sha256" \
        || err "failed to download $checksum_url"

    info "Verifying SHA-256 ..."
    verify_checksum "$tmp_dir/$asset" "$tmp_dir/$asset.sha256"

    info "Extracting ..."
    tar -xzf "$tmp_dir/$asset" -C "$tmp_dir"

    binary="$tmp_dir/$name/envprobe"
    if [ ! -f "$binary" ]; then
        err "extracted archive does not contain $name/envprobe"
    fi
    chmod 0755 "$binary"

    info "Installing to ${install_dir}/envprobe ..."
    install_binary "$binary" "$install_dir"

    info ""
    info "Installed envprobe ${version} to ${install_dir}/envprobe"

    case ":${PATH:-}:" in
        *":$install_dir:"*)
            info "Run: envprobe"
            ;;
        *)
            info "Add ${install_dir} to your PATH, then run: envprobe"
            ;;
    esac
}

main "$@"
