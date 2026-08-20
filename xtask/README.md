# xtask

Internal workspace automation for the HID Usage Tables catalog. This package is
not published.

The binary has no third-party dependencies. SHA-256, JSON parsing, and catalog
generation are implemented in this crate so `cargo xtask verify` can run
offline.

## Examples

From the workspace root:

```console
cargo xtask generate --check
cargo xtask generate
cargo xtask verify
cargo xtask stats
```

Extract or update from a USB-IF HUT PDF (requires Poppler `pdfdetach`):

```console
cargo xtask extract path/to/hut.pdf
cargo xtask update path/to/hut.pdf \
  --publication-date YYYY-MM-DD \
  --url https://www.usb.org/path/to/new-hut.pdf
```

`update` rewrites the provenance manifest and catalog transactionally and rolls
all changed files back if any step fails. Review the CSV supplements after every
specification bump: reserved page ranges, aliases, external pages, and
deprecation annotations are not all present in the embedded JSON.
