---
title: SOMME
section: 1
header: User Commands
footer: Somme 0.1.0
date: 2026-08-10
---

# NAME

somme - shared authenticated command-line foundation for Somme applications

# SYNOPSIS

`somme [-a|--app APP] [-e|--env-prefix PREFIX] [-u|--api-base URL] COMMAND`

# DESCRIPTION

Somme provides account-profile persistence, bearer-authenticated JSON requests, weighted fair-use reporting, structured API errors, and installable manuals for derived product CLIs. Product-specific commands belong in derived clients such as Suffix and Bay. Secrets are stored in the platform configuration directory and are never printed by configuration commands.

# GLOBAL OPTIONS

`-a, --app APP`
: Configuration namespace. Default: `somme`.

`-e, --env-prefix PREFIX`
: Prefix used for `PREFIX_API_KEY` and `PREFIX_API_BASE`. Default: `SOMME`.

`-u, --api-base URL`
: Default API base for newly saved accounts.

`-h, --help`
: Print help.

`-V, --version`
: Print the version.

# COMMANDS

## login

`somme login [-t|--token TOKEN] [-u|--api-base URL] [-a|--account NAME] [-e|--email EMAIL] [-r|--tier TIER]`

Save a token and select its profile. Without `--token`, select an already-authenticated saved account. `--account` defaults to the email, active account, or `default`. `--api-base` overrides the product default. `--email` and `--tier` cache non-secret account metadata.

## logout

`somme logout [-a|--account NAME]`

Remove the selected account's token while preserving its profile.

## account

`somme account [ls|use NAME]`

List saved profiles or select one. The active profile is marked with `*`.

## config

`somme config`

Print the namespace, active account, configuration path, and count of stored tokens without printing secrets.

## request

`somme request PATH [-j|--json]`

Send an authenticated GET request to `PATH`. The compact JSON response goes to standard output; `--json` pretty-prints it. Response metadata stays on standard error so pipelines receive only the requested JSON. When supplied by the server, Somme reports the operation cost, remaining and total allowance, reset epoch, retry interval, account tier, request id, and fair-use warning.

An HTTP 429 response exits nonzero without printing a successful response body. The error retains the server's message and reports its structured fair-use scope, exact cost, remaining allowance, shortfall, retry time, request id, and lower-cost alternatives. Alternatives are advice only: Somme never silently reduces a request or retries a write. Product CLIs may inspect the typed `ApiError` and apply their own explicitly documented retry policy for idempotent reads.

## man

`somme man [show]`

Print the complete embedded section-1 manual.

`somme man install [-d|--dir DIR]`

Install `somme.1`. Without `--dir`, Somme tries standard Homebrew, local, user-local, and system man directories as appropriate for the platform.

# ACCOUNTS AND CONFIGURATION

The configuration file is `somme/config.toml` below the platform configuration directory unless `--app` changes the namespace. It contains an active profile and named account records with API base, bearer token, optional email and tier, and update time. Command-line product options take precedence over saved defaults. Environment credentials take precedence when a derived client uses `Product::environment_account`.

# WEIGHTED FAIR USE

Somme understands these response headers:

- `X-RateLimit-Limit`: allowance for the reported scope, or the literal `unlimited` when the server grants a bypass for that scope.
- `X-RateLimit-Remaining`: units remaining.
- `X-RateLimit-Reset`: Unix epoch when allowance begins to replenish or reset.
- `X-RateLimit-Cost`: units charged for this operation.
- `Retry-After`: seconds or an HTTP date supplied with a temporary denial.
- `X-RateLimit-Warning`: human-readable low-quota or fair-use warning.
- `X-RateLimit-Tier`: server-assigned quota tier.
- `X-Request-Id`: identifier for support and audit correlation.

Enforcement, costs, tiers, and alternatives are selected by the server. Clients cannot lower a cost or grant themselves an unlimited scope. An unlimited daily allowance may still coexist with minute, concurrency, response-size, or other product limits.

Structured error bodies may put `code`, `scope`, `cost`, `remaining`, `shortfall`, `retryAt`, and `alternatives` at the top level or inside `fair_use`/`fairUse`. Somme preserves the complete JSON body. Each alternative may include a kind, label, exact cost, command hint, parameters, and product-specific fields.

# EXAMPLES

Save and use a token:

```
somme login -a work -t somme_sk_example -u https://api.example.test
somme request -j /v1/profile
```

A successful request can produce JSON on standard output and metadata such as this on standard error:

```text
quota: cost 25 units; 975/1000 remaining; reset epoch 1786233600; tier member; request req_123
warning: Daily allowance is below 20 percent
```

A denied request names explicit choices without performing one:

```text
Error: 429 Too Many Requests: This request needs 40 units; 20 remain.
quota: cost 40 units; 20/1000 remaining; retry after 60; request req_429
fair use: scope daily; shortfall 20
retry at: 2026-08-11T02:14:00Z
alternative: Return 25 rows (cost 10 units; command: somme request /v1/entities?limit=25)
```

Switch profiles and install the manual:

```
somme account use work
somme man install -d ~/.local/share/man/man1
man somme
```

# ENVIRONMENT

`PREFIX_API_KEY`
: Bearer token when a derived client uses the configured environment prefix.

`PREFIX_API_BASE`
: API base paired with the environment token.

# FILES

`~/.APP`
: Extensible TOML configuration containing named account profiles. A sole profile is selected automatically; multiple profiles require an explicit active account. Existing platform `APP/config.toml` configuration is read as a migration fallback until the next save.

# EXIT STATUS

Zero indicates success. Nonzero indicates invalid arguments, missing configuration, authentication or HTTP failure, fair-use or quota exhaustion, serialization failure, or manual installation failure. A lower-cost alternative is never selected implicitly.

# SEE ALSO

`suffix(1)`, `bay(1)`, and <https://github.com/querygraph/somme>.
