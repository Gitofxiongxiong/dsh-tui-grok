# M0/M1 parity fixtures

These fixtures are the migration baseline for the current fallback UI and the
DSH-neutral projection contract. They are semantic inputs, not ANSI recordings:
the runner compares stable entry ids, block kinds, queue/interaction identity,
terminal size, and expected fallback labels.

Required scenarios are listed in `manifest.json`:

`empty`, `user-assistant`, `streaming-tool`, `queue`, `approval-question`,
`two-sessions`, `reconnect`, and `narrow-terminal`.

The fallback screen baselines are intentionally labelled `fallback`; they are
not Grok reference goldens and must be replaced by the reference runner in M10.
