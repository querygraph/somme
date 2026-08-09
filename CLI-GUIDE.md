---
title: SOMME
section: 1
header: User Commands
footer: Somme 0.1.0
date: 2026-08-08
---

# NAME

somme - shared authenticated command-line foundation for Somme applications

# SYNOPSIS

`somme [-a|--app APP] [-e|--env-prefix PREFIX] [-u|--api-base URL] COMMAND`

# DESCRIPTION

Somme provides account-profile persistence, bearer-authenticated JSON requests, rate-limit reporting, and installable manuals for derived product CLIs. Product-specific commands belong in derived clients such as Suffix and Bay. Secrets are stored in the platform configuration directory and are never printed by configuration commands.

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

Send an authenticated GET request to `PATH`. The compact JSON response goes to standard output; `--json` pretty-prints it. Finite quota information is printed to standard error. HTTP 429 failures include the reset epoch when supplied by the server.

## man

`somme man [show]`

Print the complete embedded section-1 manual.

`somme man install [-d|--dir DIR]`

Install `somme.1`. Without `--dir`, Somme tries standard Homebrew, local, user-local, and system man directories as appropriate for the platform.

# ACCOUNTS AND CONFIGURATION

The configuration file is `somme/config.toml` below the platform configuration directory unless `--app` changes the namespace. It contains an active profile and named account records with API base, bearer token, optional email and tier, and update time. Command-line product options take precedence over saved defaults. Environment credentials take precedence when a derived client uses `Product::environment_account`.

# RATE LIMITS

Somme reads `X-RateLimit-Limit`, `X-RateLimit-Remaining`, and `X-RateLimit-Reset`. The literal value `unlimited` identifies an administrator bypass. Enforcement is server-side; clients cannot grant themselves administrator status.

# EXAMPLES

Save and use a token:

```
somme login -a work -t somme_sk_example -u https://api.example.test
somme request -j /v1/profile
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

`APP/config.toml`
: Named account profiles under the platform configuration directory.

# EXIT STATUS

Zero indicates success. Nonzero indicates invalid arguments, missing configuration, authentication or HTTP failure, rate exhaustion, serialization failure, or manual installation failure.

# SEE ALSO

`suffix(1)`, `bay(1)`, and <https://github.com/querygraph/somme>.
