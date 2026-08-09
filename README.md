# Somme

Somme is the shared foundation for a family of authenticated CLIs and APIs. It keeps product commands in derived distributions while centralizing account profiles, bearer requests, rate-limit reporting, API-key hashing, UTC quota metadata, and the explicit administrator bypass.

The CLI-family contract lives in `AGENTS.md`: optional arguments have paired short and long forms, each derived CLI checks in a complete `CLI-GUIDE.md` and matching section-1 man page, and each CLI supports `man show` and `man install`.

- `somme-cli` is the Rust crate and neutral `somme` binary.
- `@querygraph/somme` provides backend-safe JavaScript primitives.
- `suffix` derives its existing shortcut/domain commands from this crate.
- `bay` derives devreal community-graph commands from this crate.

Finite users are rate-limited by the product backend. A server-verified administrator receives `X-RateLimit-Limit: unlimited`; clients cannot request or forge this bypass.

```sh
cargo test
cargo clippy -- -D warnings
npm test
./scripts/generate-man.sh
mandoc -T utf8 man/somme.1 >/dev/null
```
