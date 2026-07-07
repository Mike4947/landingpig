#!/usr/bin/env bash
# landingpig installer wizard — Linux, macOS, and Windows (Git Bash / MSYS2 / WSL)
set -euo pipefail

VERSION="0.1.0"
REQUIRED_KB=51200
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OS=""
INSTALL_ROOT=""
BIN_DIR=""
EXE_NAME="landingpig"
PATH_MARKER="# landingpig installer"

SPACE_ERROR='Sorry, you don'\''t have enough space on your drive to install the big fat pig on your machine, choose another drive or make him some space for his big butt'

# ── UI helpers ────────────────────────────────────────────────────────────────

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
cyan() { printf '\033[36m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }
red() { printf '\033[31m%s\033[0m\n' "$*"; }

step() {
  echo
  bold "[$1/5] $2"
  echo "────────────────────────────────────────"
}

press_enter() {
  printf '\nPress Enter to continue...'
  read -r _
}

# ── Platform detection ────────────────────────────────────────────────────────

detect_os() {
  local kernel
  kernel="$(uname -s 2>/dev/null || echo unknown)"

  case "$kernel" in
    Linux*)
      if grep -qiE 'microsoft|wsl' /proc/version 2>/dev/null; then
        OS="wsl"
      else
        OS="linux"
      fi
      ;;
    Darwin*) OS="macos" ;;
    MINGW*|MSYS*|CYGWIN*) OS="windows" ;;
    *)
      red "Unsupported operating system: $kernel"
      exit 1
      ;;
  esac
}

default_install_root() {
  case "$OS" in
    windows)
      local drive="${SYSTEMDRIVE:-C:}"
      drive="${drive%/}"
      if command -v cygpath >/dev/null 2>&1; then
        cygpath "${drive}/landingpig"
      else
        local letter="${drive%:}"
        letter="$(printf '%s' "$letter" | tr '[:upper:]' '[:lower:]')"
        echo "/${letter}/landingpig"
      fi
      ;;
    wsl)
      if [[ -d /mnt/c ]]; then
        echo "/mnt/c/landingpig"
      else
        echo "${HOME}/landingpig"
      fi
      ;;
    macos)
      echo "/landingpig"
      ;;
    linux)
      if [[ -d /mnt/c ]]; then
        echo "/mnt/c/landingpig"
      else
        echo "/landingpig"
      fi
      ;;
  esac
}

win_path() {
  local unix_path="$1"
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -w "$unix_path"
  elif [[ "$unix_path" =~ ^/([a-zA-Z])/(.*)$ ]]; then
    local drive="${BASH_REMATCH[1]}"
    local rest="${BASH_REMATCH[2]}"
    rest="${rest//\//\\}"
    printf '%s:\\%s' "$(printf '%s' "$drive" | tr '[:lower:]' '[:upper:]')" "$rest"
  else
    printf '%s' "$unix_path"
  fi
}

# ── Disk space ────────────────────────────────────────────────────────────────

available_kb() {
  local target="$1"
  local parent
  parent="$(dirname "$target")"
  mkdir -p "$parent" 2>/dev/null || true
  if [[ ! -d "$parent" ]]; then
    parent="${HOME}"
  fi

  if df -Pk "$parent" >/dev/null 2>&1; then
    df -Pk "$parent" | awk 'NR==2 {print $4}'
    return
  fi
  if command -v powershell.exe >/dev/null 2>&1; then
    local win_parent
    win_parent="$(win_path "$parent")"
    powershell.exe -NoProfile -Command \
      "(Get-PSDrive -Name ($env:SystemDrive).TrimEnd(':') | Select-Object -ExpandProperty Free)/1KB" 2>/dev/null \
      | tr -d '\r' || echo 0
    return
  fi
  echo 999999999
}

check_space() {
  local avail
  avail="$(available_kb "$INSTALL_ROOT")"
  if [[ -z "$avail" ]] || [[ "$avail" -lt "$REQUIRED_KB" ]]; then
    red "$SPACE_ERROR"
    yellow "Required: $((REQUIRED_KB / 1024)) MB free · Available: $(( ${avail:-0} / 1024 )) MB"
    exit 1
  fi
  green "Disk space OK ($(( avail / 1024 )) MB available)."
}

# ── Build ─────────────────────────────────────────────────────────────────────

ensure_rust() {
  if command -v cargo >/dev/null 2>&1; then
    return
  fi
  yellow "Rust toolchain not found."
  cyan "Install Rust from https://rustup.rs then re-run this installer."
  exit 1
}

build_binary() {
  ensure_rust
  cyan "Building landingpig (release)..."
  (cd "$SCRIPT_DIR" && cargo build --release)
  local built="${SCRIPT_DIR}/target/release/${EXE_NAME}"
  if [[ "$OS" == "windows" ]] && [[ ! -f "$built" ]]; then
    built="${SCRIPT_DIR}/target/release/${EXE_NAME}.exe"
    EXE_NAME="landingpig.exe"
  fi
  if [[ ! -f "$built" ]]; then
    red "Build failed — binary not found at target/release/${EXE_NAME}"
    exit 1
  fi
  echo "$built"
}

# ── Install files ─────────────────────────────────────────────────────────────

install_files() {
  local built_bin="$1"
  mkdir -p "$BIN_DIR"
  cp -f "$built_bin" "${BIN_DIR}/${EXE_NAME}"
  chmod +x "${BIN_DIR}/${EXE_NAME}" 2>/dev/null || true

  [[ -f "${SCRIPT_DIR}/README.md" ]] && cp -f "${SCRIPT_DIR}/README.md" "${INSTALL_ROOT}/README.md"
  echo "$VERSION" > "${INSTALL_ROOT}/VERSION"

  cat > "${INSTALL_ROOT}/install-info.txt" <<EOF
landingpig ${VERSION}
Installed: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
Platform: ${OS}
Binary: ${BIN_DIR}/${EXE_NAME}
Config: ~/.config/landingpig/config.json
EOF
}

# ── PATH setup ────────────────────────────────────────────────────────────────

path_line_unix() {
  printf 'export PATH="%s:$PATH" %s\n' "$BIN_DIR" "$PATH_MARKER"
}

add_path_unix() {
  local line
  line="$(path_line_unix)"
  local updated=0
  for rc in "${HOME}/.bashrc" "${HOME}/.bash_profile" "${HOME}/.zshrc" "${HOME}/.profile"; do
    [[ -f "$rc" ]] || touch "$rc"
    if grep -qF "$PATH_MARKER" "$rc" 2>/dev/null; then
      sed -i.bak "s|.*${PATH_MARKER}.*|${line}|" "$rc" 2>/dev/null \
        || sed -i '' "s|.*${PATH_MARKER}.*|${line}|" "$rc"
    else
      printf '\n%s\n' "$line" >> "$rc"
    fi
    updated=1
  done
  if [[ "$updated" -eq 1 ]]; then
    green "Added ${BIN_DIR} to shell PATH in your profile."
  fi
  export PATH="${BIN_DIR}:$PATH"
}

add_path_windows() {
  local win_bin
  win_bin="$(win_path "$BIN_DIR")"
  if command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -Command "
      \$dir = '${win_bin}'
      \$user = [Environment]::GetEnvironmentVariable('Path','User')
      if (\$user -notlike \"*\$dir*\") {
        [Environment]::SetEnvironmentVariable('Path', \"\$dir;\" + \$user, 'User')
      }
    " >/dev/null 2>&1 || true
    green "Added ${win_bin} to your Windows user PATH."
    export PATH="${BIN_DIR}:$PATH"
    return
  fi
  add_path_unix
}

setup_path() {
  case "$OS" in
    windows) add_path_windows ;;
    *)       add_path_unix ;;
  esac
}

# ── Wizard ────────────────────────────────────────────────────────────────────

welcome() {
  clear 2>/dev/null || true
  bold "╔══════════════════════════════════════════════════════════╗"
  bold "║           landingpig installer wizard v${VERSION}           ║"
  bold "╚══════════════════════════════════════════════════════════╝"
  echo
  cyan "This wizard installs the landingpig CLI on your main drive"
  cyan "so you can type 'landingpig' from any terminal."
  echo
  press_enter
}

choose_location() {
  local default
  default="$(default_install_root)"
  echo
  bold "Install location"
  echo "Default (main drive): ${default}"
  printf 'Press Enter for default, or type a custom path: '
  read -r custom
  if [[ -n "${custom// }" ]]; then
    INSTALL_ROOT="${custom%/}"
  else
    INSTALL_ROOT="$default"
  fi
  BIN_DIR="${INSTALL_ROOT}/bin"

  if [[ ! -w "$(dirname "$INSTALL_ROOT")" ]] && [[ "$OS" != "windows" ]]; then
    if command -v sudo >/dev/null 2>&1; then
      yellow "Administrator permission required for ${INSTALL_ROOT}"
      sudo mkdir -p "$INSTALL_ROOT"
      sudo chown -R "$(id -u):$(id -g)" "$INSTALL_ROOT"
    else
      red "Cannot write to ${INSTALL_ROOT}. Choose another path or run with sudo."
      exit 1
    fi
  fi
}

run_wizard() {
  welcome

  step 1 "Detecting platform"
  detect_os
  green "Detected: ${OS}"

  step 2 "Choosing install location"
  choose_location
  green "Install root: ${INSTALL_ROOT}"

  step 3 "Checking disk space"
  check_space

  step 4 "Building & installing"
  local built
  built="$(build_binary)"
  install_files "$built"
  green "Installed to ${BIN_DIR}/${EXE_NAME}"

  step 5 "Configuring PATH"
  setup_path

  echo
  bold "╔══════════════════════════════════════════════════════════╗"
  green  "║  landingpig installed successfully!                      ║"
  bold "╚══════════════════════════════════════════════════════════╝"
  echo
  cyan "Open a new terminal anywhere and run:"
  bold "  landingpig"
  echo
  yellow "Config is stored at: ~/.config/landingpig/config.json"
  yellow "On first launch, paste your Anthropic API key to get started."
  echo
}

# ── Entry ─────────────────────────────────────────────────────────────────────

usage() {
  cat <<EOF
Usage: ./install.sh [options]

Options:
  --prefix <path>   Install to <path>/bin instead of the default main-drive path
  -h, --help        Show this help

Supports: Linux, macOS, Windows (Git Bash, MSYS2, Cygwin, WSL)
EOF
}

main() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --prefix)
        shift
        INSTALL_ROOT="${1:?missing path for --prefix}"
        INSTALL_ROOT="${INSTALL_ROOT%/}"
        BIN_DIR="${INSTALL_ROOT}/bin"
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        red "Unknown option: $1"
        usage
        exit 1
        ;;
    esac
  done

  if [[ -z "$INSTALL_ROOT" ]]; then
    detect_os
    run_wizard
  else
    detect_os
    check_space
    built="$(build_binary)"
    install_files "$built"
    setup_path
    green "landingpig installed to ${BIN_DIR}/${EXE_NAME}"
  fi
}

main "$@"
