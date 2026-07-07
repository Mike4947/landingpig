#!/usr/bin/env bash
# landingpig uninstaller — Linux, macOS, and Windows (Git Bash / MSYS2 / WSL)
#
UNINSTALLER_URL="${LANDINGPIG_UNINSTALLER_URL:-https://raw.githubusercontent.com/Mike4947/landingpig/main/uninstall.sh}"
if [ -z "${BASH_VERSION:-}" ]; then
  if command -v bash >/dev/null 2>&1 && command -v curl >/dev/null 2>&1; then
    exec bash -c "$(curl -fsSL "$UNINSTALLER_URL")"
  fi
  echo "Error: bash is required. Use:" >&2
  echo "  curl -fsSL $UNINSTALLER_URL | bash" >&2
  exit 1
fi

set -euo pipefail

PATH_MARKER="# landingpig installer"
CONFIG_DIR="${HOME}/.config/landingpig"
PURGE_CONFIG="${LANDINGPIG_PURGE_CONFIG:-0}"
OS=""

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
cyan() { printf '\033[36m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }
red() { printf '\033[31m%s\033[0m\n' "$*"; }

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
    *) OS="unix" ;;
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

path_from_profile_line() {
  local line="$1"
  if [[ "$line" =~ export\ PATH=\"([^\"]+) ]]; then
    local entry="${BASH_REMATCH[1]}"
    entry="${entry%%:\$PATH*}"
    entry="${entry%%:\$PATH}"
    if [[ "$entry" == */bin ]]; then
      dirname "$entry"
      return
    fi
  fi
  return 1
}

collect_install_roots() {
  local rc line root candidate bin_path

  for rc in "${HOME}/.bashrc" "${HOME}/.bash_profile" "${HOME}/.zshrc" "${HOME}/.profile"; do
    [[ -f "$rc" ]] || continue
    while IFS= read -r line || [[ -n "$line" ]]; do
      [[ "$line" == *"$PATH_MARKER"* ]] || continue
      if root="$(path_from_profile_line "$line")"; then
        printf '%s\n' "$root"
      fi
    done <"$rc"
  done

  for candidate in \
    "/landingpig" \
    "/mnt/c/landingpig" \
    "${HOME}/landingpig" \
    "/c/landingpig"; do
    [[ -f "${candidate}/install-info.txt" ]] && printf '%s\n' "$candidate"
  done

  if command -v landingpig >/dev/null 2>&1; then
    bin_path="$(command -v landingpig)"
    bin_path="$(readlink -f "$bin_path" 2>/dev/null || realpath "$bin_path" 2>/dev/null || printf '%s' "$bin_path")"
    if [[ "$bin_path" == */bin/landingpig* ]]; then
      printf '%s\n' "$(dirname "$(dirname "$bin_path")")"
    fi
  fi
}

remove_path_unix() {
  local removed=0
  local rc
  for rc in "${HOME}/.bashrc" "${HOME}/.bash_profile" "${HOME}/.zshrc" "${HOME}/.profile"; do
    [[ -f "$rc" ]] || continue
    if grep -qF "$PATH_MARKER" "$rc" 2>/dev/null; then
      grep -vF "$PATH_MARKER" "$rc" >"${rc}.landingpig.tmp"
      mv "${rc}.landingpig.tmp" "$rc"
      removed=1
    fi
  done
  if [[ "$removed" -eq 1 ]]; then
    green "Removed landingpig from shell PATH."
  fi
}

remove_path_windows() {
  local roots=("$@")
  local win_bin root win_root removed=0
  if ! command -v powershell.exe >/dev/null 2>&1; then
    remove_path_unix
    return
  fi
  for root in "${roots[@]}"; do
    [[ -n "$root" ]] || continue
    win_bin="$(win_path "${root}/bin")"
    powershell.exe -NoProfile -Command "
      \$dir = '${win_bin}'
      \$user = [Environment]::GetEnvironmentVariable('Path','User')
      if (\$user -like \"*\$dir*\") {
        \$updated = (\$user -split ';' | Where-Object { \$_ -and \$_ -ne \$dir }) -join ';'
        [Environment]::SetEnvironmentVariable('Path', \$updated, 'User')
      }
    " >/dev/null 2>&1 || true
    removed=1
  done
  remove_path_unix
  if [[ "$removed" -eq 1 ]]; then
    green "Removed landingpig from Windows user PATH."
  fi
}

remove_install_root() {
  local root="$1"
  if [[ -z "$root" || "$root" == "/" || "$root" == "$HOME" ]]; then
    yellow "Skipped unsafe path: ${root:-<empty>}"
    return
  fi
  if [[ ! -d "$root" ]]; then
    return
  fi
  if [[ ! -f "${root}/install-info.txt" && ! -f "${root}/bin/landingpig" && ! -f "${root}/bin/landingpig.exe" ]]; then
    yellow "Skipped ${root} (does not look like a landingpig install)."
    return
  fi
  rm -rf "$root"
  green "Removed ${root}"
}

main() {
  detect_os

  bold "landingpig uninstaller"
  echo

  local roots_raw roots=() root
  roots_raw="$(collect_install_roots | sort -u | grep -v '^$' || true)"
  if [[ -z "$roots_raw" ]]; then
    yellow "No landingpig installation found."
    remove_path_unix
    exit 0
  fi
  while IFS= read -r root; do
    [[ -n "$root" ]] && roots+=("$root")
  done <<<"$roots_raw"

  cyan "Found installation(s):"
  local root
  for root in "${roots[@]}"; do
    echo "  - ${root}"
  done
  echo

  for root in "${roots[@]}"; do
    remove_install_root "$root"
  done

  case "$OS" in
    windows) remove_path_windows "${roots[@]}" ;;
    *)       remove_path_unix ;;
  esac

  if [[ "$PURGE_CONFIG" == "1" && -d "$CONFIG_DIR" ]]; then
    rm -rf "$CONFIG_DIR"
    green "Removed config at ${CONFIG_DIR}"
  else
    yellow "Config kept at ${CONFIG_DIR} (set LANDINGPIG_PURGE_CONFIG=1 to remove)"
  fi

  echo
  bold "landingpig uninstalled."
  cyan "Open a new terminal for PATH changes to take effect."
}

main "$@"
