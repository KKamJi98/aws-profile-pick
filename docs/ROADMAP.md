# Roadmap

Features considered after the 3.0.0 Rust rewrite. Nothing here is
implemented yet; the order inside each tier is not a priority order.

## P2

- Arrow-key and fuzzy selection UI, as an alternative to typing a number or
  a name.
- `--no-rc`: print the export line and write the shared profile file, but do
  not edit the rc file.
- `--current`: print the current profile and exit.
- `--unset`: remove the managed `AWS_PROFILE` line (and the shared profile
  file) instead of setting it.
- `--json`: print the profile list (name, group, current) as JSON.
- nushell support.
- Profile names in shell completions (the scripts from `--completions`
  complete options only).

## P3

- Windows: PowerShell output and a Windows build target.
- Run `aws sso login` when the selected profile's SSO session has expired.
- Revisit the default for 4.0: whether editing the rc file should become
  opt-in, with the shared profile file and `eval` as the default path.
