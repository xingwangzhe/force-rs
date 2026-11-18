# Changelog

## 0.3.2 — 2026-11-18

- Split native bindings into platform-specific scoped npm packages; the root package now ships the matching platform dependency only.
- Publish platform packages before the root package through the release workflow.

## 0.3.1 — 2026-11-18

- Upgrade napi-rs and compatible transitive Rust dependencies.
- Verify native force simulation tests and lint locally.
