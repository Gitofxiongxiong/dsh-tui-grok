#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./dsh-tui-common.sh
source "$script_dir/dsh-tui-common.sh"

repo_root="$(dsh_tui_repo_root "$script_dir")"
tui_profile="${DSH_TUI_PROFILE:-$dsh_tui_default_profile}"
dsh_tui_validate_profile_name "$tui_profile"

usage() {
  printf '%s\n' \
    'Usage: scripts/setup-dev-profile.sh' \
    '' \
    'Builds the local TypeScript packages and installs them into a DSH profile.' \
    '' \
    'Environment overrides:' \
    '  DSH_HARNESS_ROOT            DeepSeek Harness checkout' \
    '  DSH_TUI_PROFILE             profile name (default: dsh-pager-grok-controllers-v2-dev)' \
    '  DSH_TUI_PROFILE_ALLOW_UPDATE=1' \
    '                               allow updating an existing non-project profile' \
    '  DSH_HOME                    Harness home (default: $HOME/.dsh)'
}

case "${1:-}" in
  '') ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

node_program="$(dsh_tui_resolve_node)"
harness_root="$(dsh_tui_resolve_harness_root "$repo_root")"
harness_entry="$(dsh_tui_require_harness_entry "$harness_root")"

dsh_tui_ensure_workspace_dependencies "$repo_root"
dsh_tui_ensure_typescript_build "$repo_root"

# The CLI invokes pnpm for the profile itself; fail early with a local message
# instead of letting a missing package manager surface as a Node stack trace.
dsh_tui_pnpm --version >/dev/null

printf 'Using Node.js: %s\n' "$node_program" >&2
printf 'Using DeepSeek Harness: %s\n' "$harness_root" >&2
printf 'Using DSH_HOME: %s\n' "$(dsh_tui_dsh_home)" >&2

# plugin add is invoked as node + absolute bin.js (not a shebang exec of .js).
dsh_tui_install_local_profile "$repo_root" "$harness_root" "$harness_entry" "$tui_profile"
