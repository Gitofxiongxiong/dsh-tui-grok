# DSH 0.1.1-rc.2 compatibility fixture

This directory is the isolated npm composition for the `apiproxy-v1` family.
It deliberately lives outside the pnpm workspace, pins every directly used DSH
package to `0.1.1-rc.2`, and never relies on the root alpha checkout overrides.

Run from the repository root:

```sh
corepack pnpm@11.7.0 --pm-on-fail=ignore \
  --dir compat/fixtures/dsh-0.1.1-rc.2 install --frozen-lockfile
corepack pnpm@11.7.0 --pm-on-fail=ignore \
  --dir compat/fixtures/dsh-0.1.1-rc.2 run build
corepack pnpm@11.7.0 --pm-on-fail=ignore \
  --dir compat/fixtures/dsh-0.1.1-rc.2 run e2e
```

The E2E runner creates a project-named profile inside a fresh `/tmp` DSH_HOME,
executes hello/list/dashboard/load and the PTY smoke, then removes that home.
Set `KEEP_DSH_HOME=1` only for local diagnosis; the path is printed before any
profile mutation. The fixture imports the repository's built adapter/core by
relative path, so `pnpm run build:ts` must succeed first.
