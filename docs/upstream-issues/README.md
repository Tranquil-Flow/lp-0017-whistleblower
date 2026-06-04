# Upstream issue drafts (ready to file)

These are the upstream issues encountered while building LP-0017, written up
ready to file against the correct Logos repos. They mirror the running log in
[`../../BUGS_FILED.md`](../../BUGS_FILED.md) (which keeps the full context); the
files here are formatted for direct paste into `gh issue create`.

**Status:** drafts. Filing is a deliberate, outward-facing step done by the
submitter, not automated. File with e.g.:

```bash
gh issue create --repo logos-co/logos-delivery-module \
  --title "$(sed -n 's/^# //p' 03-logos-delivery-module-librln-missing.md | head -1)" \
  --body-file <(sed '1,/^---$/d' 03-logos-delivery-module-librln-missing.md)
```

Once filed, record each issue URL in `../../BUGS_FILED.md` so the LP-0017
"GitHub issues filed for problems with Logos technology" criterion is backed by
live links.

| Draft | Target repo | Severity |
|---|---|---|
| `01-logos-scaffold-template-runner-api-rot.md` | `logos-co/logos-scaffold` | low |
| `02-logos-liblogos-gtest-timeout.md` | `logos-co/logos-liblogos` | moderate |
| `03-logos-delivery-module-librln-missing.md` | `logos-co/logos-delivery-module` | high |
| `04-spel-idlseed-hashed-seed.md` | `logos-co/spel` | moderate |
| `05-lez-testnet-cu-not-persisted.md` | `logos-blockchain/logos-execution-zone` | moderate |

Already filed: `logos-blockchain/logos-blockchain-circuits#33` (closed by maintainer).
