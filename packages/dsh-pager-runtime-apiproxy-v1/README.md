# `@dsh-pager-grok/runtime-apiproxy-v1`

Publishable family runtime for exact DSH `0.1.1-rc.2`. The
`@dsh-pager-grok/cli` resolver selects this package through
`compat/dsh-support.json`; users do not select the internal profile directly.

The tarball contains the stable protocol/core and only the `apiproxy-v1`
adapter. Its profile patch is derived from the independently exercised rc.2
fixture and uses official rc.2 storage/session-projection-cache packages. It
does not contain the retired pager projection-recovery plugin or any
controllers-v2/alpha dependency.
