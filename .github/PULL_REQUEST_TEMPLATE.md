## What it does

<!-- What changes for someone using Flow, or for the code. Link the issue it closes. -->

## Why

<!-- What it's for. For a feature, link its design in docs/design/. -->

## Checked

<!-- What you ran and tried, on which systems, and what wasn't built there:
     "cargo test --workspace and clippy on macOS 26 and Ubuntu 24.04; not built on Windows". -->

- [ ] `cargo test --workspace` passes on: <!-- macOS / Windows / Linux -->
- [ ] `cargo clippy --workspace` adds no warnings
- [ ] Not run through `cargo fmt`; the code matches its surroundings by hand
- [ ] New tests are named as sentences, and comments say why
- [ ] Every new string a user reads is in both `core/locales/en.json` and `core/locales/ja.json`
- [ ] The matching page in `docs/manual/` is updated, for a setting, shortcut, config key, style or platform behavior
- [ ] For a change to what Flow draws: a screenshot or snapshot, showing no user or host name
