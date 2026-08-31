#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=./dsh-tui-common.sh
source "$(dirname "$0")/dsh-tui-common.sh"

tui_profile="${DSH_TUI_PROFILE:-$dsh_tui_default_profile}"
dsh_tui_validate_profile_name "$tui_profile"
timeout_seconds="${REAL_E2E_TIMEOUT:-45}"
session_id="${REAL_E2E_SESSION:-}"
install_local="${DSH_TUI_INSTALL_LOCAL:-0}"

if [[ "$install_local" == "1" && -n "${DSH_TUI_SERVER:-}" ]]; then
  printf 'DSH_TUI_INSTALL_LOCAL=1 cannot be combined with a custom DSH_TUI_SERVER\n' >&2
  exit 2
fi

use_env_backend=0
if [[ -n "${DSH_TUI_SERVER:-}" ]]; then
  use_env_backend=1
  harness_root="$(dsh_tui_resolve_harness_root "$repo_root")"
  backend_desc="$DSH_TUI_SERVER"
  # pty-smoke.py always passes --backend (default node+mock). Translate the
  # whitespace-split override into argv; paths still cannot contain spaces.
  # shellcheck disable=SC2206
  server_parts=($DSH_TUI_SERVER)
  if (( ${#server_parts[@]} == 0 )); then
    printf 'DSH_TUI_SERVER is empty after whitespace split.\n' >&2
    exit 2
  fi
  pty_backend_argv=(--backend "${server_parts[0]}")
  for ((i = 1; i < ${#server_parts[@]}; i++)); do
    pty_backend_argv+=(--backend-arg "${server_parts[i]}")
  done
else
  if [[ "$install_local" == "1" ]]; then
    "$repo_root/scripts/setup-dev-profile.sh"
  else
    dsh_tui_require_profile "$tui_profile" "$repo_root"
  fi
  dsh_tui_prepare_pager_backend "$repo_root" "$tui_profile"
  harness_root="$dsh_tui_harness_root"
  backend_desc="$dsh_tui_node_program $dsh_tui_harness_entry --profile $tui_profile"
  # argparse treats a following `--profile` token as another option even when
  # it is intended as the value of `--backend-arg`; use the equals form for
  # every backend argument so option-shaped child argv remains unambiguous.
  pty_backend_argv=(
    --backend "$dsh_tui_node_program"
    "--backend-arg=$dsh_tui_harness_entry"
    "--backend-arg=--profile"
    "--backend-arg=$tui_profile"
  )
fi

cd "$repo_root"
cargo build -p dsh-pager-bin --locked

binary="$repo_root/target/debug/dsh-pager"

run_pager() {
  timeout "${timeout_seconds}s" "$binary" "$@"
}

printf 'real Harness root: %s\n' "$harness_root"
printf 'real Harness command: %s\n' "$backend_desc"
if (( use_env_backend == 1 )); then
  run_pager --hello
  run_pager --list-sessions
  run_pager --dashboard
else
  run_pager --hello "${dsh_tui_pager_backend_argv[@]}"
  run_pager --list-sessions "${dsh_tui_pager_backend_argv[@]}"
  run_pager --dashboard "${dsh_tui_pager_backend_argv[@]}"
fi

pager_session_args=()
if [[ -n "$session_id" ]]; then
  pager_session_args=(--resume "$session_id")
  printf 'using existing session: %s\n' "$session_id"
else
  printf 'using the default isolated new session (set REAL_E2E_SESSION for read-only attach)\n'
fi
if (( use_env_backend == 1 )); then
  run_pager --load-only "${pager_session_args[@]}"
else
  run_pager --load-only "${pager_session_args[@]}" "${dsh_tui_pager_backend_argv[@]}"
fi

# The PTY smoke exercises the credential modal with a deterministic placeholder.
# Never let that fixture persist into the caller's profile: a second E2E run
# must see the same clean credential state as the first one. Copy only the
# selected profile wiring into an ephemeral DSH_HOME; sessions, credentials and
# storage are deliberately fresh.
pty_home_parent="${TMPDIR:-/tmp}"
pty_dsh_home="$(mktemp -d "$pty_home_parent/dsh-pager-real-e2e.XXXXXX")"
cleanup_pty_dsh_home() {
  if [[ -n "${pty_dsh_home:-}" \
    && -d "$pty_dsh_home" \
    && "$pty_dsh_home" == "$pty_home_parent"/dsh-pager-real-e2e.* ]]; then
    rm -rf -- "$pty_dsh_home"
  fi
}
trap cleanup_pty_dsh_home EXIT INT TERM

if (( use_env_backend == 0 )); then
  source_profile_dir="$(cd "$(dsh_tui_profile_dir "$tui_profile")" && pwd -P)"
  isolated_profile_dir="$pty_dsh_home/profiles/$tui_profile"
  mkdir -p "$isolated_profile_dir"
  cp -a "$source_profile_dir/." "$isolated_profile_dir/"

  # pnpm writes relative link: symlinks whose meaning depends on the profile's
  # original directory depth.  `cp -a` preserves their raw text, so relocating
  # the profile under /tmp can turn a valid /home/.../code target into a broken
  # /tmp/code target.  Preserve the source links' resolved meaning while the
  # copied profile files, sessions, credentials and storage stay isolated.
  while IFS= read -r -d '' source_link; do
    relative_link="${source_link#"$source_profile_dir"/}"
    isolated_link="$isolated_profile_dir/$relative_link"
    canonical_target="$(realpath "$source_link")"
    unlink "$isolated_link"
    ln -s "$canonical_target" "$isolated_link"
    if [[ "$(realpath "$isolated_link")" != "$canonical_target" ]]; then
      printf 'Failed to preserve isolated profile link: %s\n' "$relative_link" >&2
      exit 2
    fi
  done < <(find "$source_profile_dir" -type l -print0)
fi

env -u DEEPSEEK_API_KEY DSH_HOME="$pty_dsh_home" \
  python3 scripts/pty-smoke.py \
  --binary "$binary" \
  "${pty_backend_argv[@]}" \
  --timeout "$timeout_seconds"

printf 'real DeepSeek Harness E2E checks passed (hello/list/dashboard/load/PTY)\n'
