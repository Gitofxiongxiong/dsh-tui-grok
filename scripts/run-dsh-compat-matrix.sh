#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
requested_version="${1:-}"
if [[ -z "$requested_version" ]]; then
  printf 'Usage: scripts/run-dsh-compat-matrix.sh <exact-version-from-compat/dsh-support.json>\n' >&2
  exit 2
fi
IFS=$'\t' read -r version family tag commit package_manager checkout_var checkout_dir source fixture label \
  < <(node "$repo_root/scripts/dsh-matrix-config.mjs" --version-tsv "$requested_version")
fixture="$repo_root/$fixture"
root_package_manager="$(node -p "require('$repo_root/package.json').packageManager")"

checkout="${!checkout_var:-}"
if [[ -z "$checkout" || ! -d "$checkout" ]] \
  || [[ "$(git -C "$checkout" rev-parse --is-inside-work-tree 2>/dev/null || true)" != "true" ]]; then
  printf '%s must point to the exact %s DSH checkout\n' "$checkout_var" "$version" >&2
  exit 2
fi

matrix_parent="${TMPDIR:-/tmp}"
matrix_home="$(mktemp -d "$matrix_parent/dsh-pager-matrix-${label}.XXXXXX")"
cleanup() {
  if [[ -n "${matrix_home:-}" && -d "$matrix_home" \
    && "$matrix_home" == "$matrix_parent"/dsh-pager-matrix-"$label".* ]]; then
    rm -rf -- "$matrix_home"
  fi
}
trap cleanup EXIT INT TERM

printf 'matrix DSH version: %s\n' "$version"
printf 'matrix exact checkout: %s\n' "$checkout"
printf 'matrix exact tag/commit: %s %s\n' "$tag" "$commit"
printf 'matrix family/source: %s %s\n' "$family" "$source"
printf 'matrix package manager: %s\n' "$package_manager"
printf 'matrix DSH_HOME: %s\n' "$matrix_home"
env "$checkout_var=$checkout" node "$repo_root/scripts/check-dsh-support.mjs"

if [[ "$source" == "source-only" ]]; then
  corepack "$package_manager" --pm-on-fail=ignore --dir "$checkout" install --frozen-lockfile
  corepack "$package_manager" --pm-on-fail=ignore --dir "$checkout" run build:lib:host
  corepack "$package_manager" --pm-on-fail=ignore --dir "$checkout" run build:lib:client
  corepack "$root_package_manager" --pm-on-fail=ignore --dir "$repo_root" install --frozen-lockfile
  DSH_HOME="$matrix_home" \
  DSH_HARNESS_ROOT="$checkout" \
  DSH_TUI_PROFILE="dsh-pager-grok-controllers-v2-${label}-matrix" \
  DSH_TUI_INSTALL_LOCAL=1 \
    "$repo_root/scripts/real-e2e.sh"
else
  corepack "$package_manager" --pm-on-fail=ignore --dir "$fixture" install --frozen-lockfile
  DSH_COMPAT_VERSION="$version" \
  DSH_COMPAT_E2E_HOME="$matrix_home" \
    corepack "$package_manager" --pm-on-fail=ignore --dir "$fixture" run e2e
fi

printf '%s compatibility matrix passed (exact checkout/install/build/real E2E/PTY)\n' "$version"
