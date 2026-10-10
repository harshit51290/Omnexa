# Security Policy

## Supported versions

Latest `main` and the newest release. Older pre-releases are not patched —
update and re-test.

## Report a vulnerability

Open a **private security advisory** on GitHub
([repo](../../security/advisories/new)) or a regular issue if it is not
sensitive. Please include:

- What you did, what you expected, what happened
- The `--details` output for the file involved (redact account IDs)
- Windows version and how you installed the app

We aim to acknowledge within a week.

## Please do NOT

- Attach full save files to public issues — they contain your Steam
  account ID. Paste `--details` output instead (it omits IDs), or a
  minimal hex snippet.
- Put API keys (SteamGridDB) in issues, PRs or config files. Keys live
  only in the app's Settings on your machine.
- Use this tool against accounts or machines you don't own.
