# Somme CLI family contract

Somme is the base for product CLIs such as Suffix and Bay. Changes to Somme or a derived CLI must preserve this interface contract.

## Options

- Every user-visible optional argument must have both a stable long option and a stable short option. Positional arguments are exempt.
- Short options only need to be unique within their command scope. Prefer mnemonic lowercase letters; use an uppercase letter when all sensible lowercase letters conflict.
- Keep a recursive parser test that walks every command and fails when a long option has no short form.
- Never silently rename or reuse an established option for a different meaning.

## Manuals

- Every Somme-derived CLI must check in `CLI-GUIDE.md` as its complete canonical manual.
- Every derived CLI must check in a section-1 man page generated from that guide. The guide and man page must describe every command, positional argument, short/long option pair, environment variable, configuration file, exit behavior, and material example.
- Provide `CLI man show` and `CLI man install [-d|--dir DIR]`. Installation should use Somme's shared man-page installer.
- `CLI-GUIDE.md` and `man/CLI.1` must change in the same commit whenever parser behavior or documented semantics change.
- Generate the man page with the repository's `scripts/generate-man.sh`; do not hand-edit generated roff.

## Verification

- Run formatting, parser tests, strict Clippy, manual generation, and a rendered-man smoke check before publishing.
- Review recursive `--help` output against `CLI-GUIDE.md`; generated documentation does not excuse missing explanations or examples.
