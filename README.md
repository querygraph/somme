# Somme

Somme is the shared foundation for a family of authenticated CLIs and APIs. It keeps product commands in derived distributions while centralizing account profiles, bearer requests, weighted fair-use reporting, structured API errors, API-key hashing, and quota metadata.

The CLI-family contract lives in `AGENTS.md`: optional arguments have paired short and long forms, each derived CLI checks in a complete `CLI-GUIDE.md` and matching section-1 man page, and each CLI supports `man show` and `man install`.

- `somme-cli` is the Rust crate and neutral `somme` binary.
- `@querygraph/somme` provides backend-safe JavaScript primitives.
- `suffix` derives its existing shortcut/domain commands from this crate.
- `bay` derives devreal community-graph commands from this crate.

Product backends select costs and limits. Somme parses `X-RateLimit-Cost`, limit, remaining, reset, `Retry-After`, warning, tier, and request-id metadata. The literal `X-RateLimit-Limit: unlimited` represents only the scope granted by the server; clients cannot request or forge it.

Rust callers receive successful JSON in `ApiResponse`. Non-success responses remain downcastable `ApiError` values inside the crate's convenient `anyhow::Result`: status, complete JSON or text body, response metadata, fair-use fields, and lower-cost alternatives are retained. Derived CLIs can use `response_metadata_lines` for consistent stderr output, `as_api_error` to inspect failures without parsing display text, and `ApiResponse::with_body` to reshape data without dropping request metadata.

The JavaScript package emits the same weighted headers through `rateLimitHeaders`. `RateLimitError` carries a structured response body so web frameworks can return the exact cost, scope, retry time, and alternatives rather than reducing work silently.

```sh
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
npm test
./scripts/generate-man.sh
mandoc -T utf8 man/somme.1 >/dev/null
```
