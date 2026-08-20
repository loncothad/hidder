# Usage-table generation and updates

The usage catalog is generated code, but the source data and every transformation input are checked into the workspace.

## Inputs

| File | Role |
|---|---|
| `spec/upstream/HidUsageTables.json` | Exact machine-readable attachment extracted from the HUT 1.7 PDF |
| `spec/source.toml` | Source revision, publication date, attachment name, SHA-256, and canonical URLs |
| `spec/pages.csv` | Page-summary records absent from the JSON (Undefined and externally specified pages) |
| `spec/page-ranges.csv` | Reserved/vendor-defined page ranges from HUT table 3.1 |
| `spec/aliases.csv` | Source, legacy, or search aliases not represented by the JSON schema |
| `spec/status.csv` | Deprecation annotations not represented by the JSON schema |

The generated output is `crates/hid-usage-tables/src/generated.rs`.

## Deterministic generation

```console
cargo xtask generate
cargo xtask generate --check
cargo xtask check
cargo xtask verify
cargo xtask stats
```

`verify` performs all of the following:

1. computes SHA-256 of the checked-in JSON and compares it to `spec/source.toml`;
2. parses the attachment with the dependency-free JSON parser;
3. confirms the version triplet;
4. regenerates the complete Rust source in memory; and
5. byte-compares it with the checked-in generated file.

Generation sorts pages and usages by numeric identifier. It rejects duplicate pages, duplicate usage IDs, malformed ranges, unknown usage-type abbreviations, aliases/status entries without a target record, overlapping explicit page ranges, and values outside 16 bits.

For finite defined pages, the generator computes the complement of named IDs over `0x0000..=0xffff` and stores those intervals as reserved/unassigned for the current source revision. Generated pages keep their source generator range instead. This distinction allows a later revision to define a previously unassigned value without changing the public representation.

## Updating from a new HUT PDF

Install Poppler utilities so `pdfdetach` is available, then run:

```console
cargo xtask update /path/to/new-hut.pdf \
  --publication-date YYYY-MM-DD \
  --url https://www.usb.org/path/to/new-hut.pdf
```

The command:

1. extracts the attachment named in `spec/source.toml` to a temporary file;
2. parses it and verifies its structural root/version;
3. computes its SHA-256;
4. saves the old JSON, manifest, and generated file in memory;
5. installs the new JSON and updates source metadata;
6. regenerates the catalog; and
7. rolls all three files back on any failure.

After a successful update, review:

- `spec/pages.csv` against the new Usage Page Summary;
- `spec/page-ranges.csv` for changed reserved or vendor-defined ranges;
- `spec/aliases.csv` for PDF spellings/legacy names not represented in JSON;
- `spec/status.csv` for new deprecations;
- newly introduced Usage Type abbreviations or JSON fields; and
- the generated diff and catalog statistics.

Then run the full workspace tests and `cargo xtask verify`.

## Why supplements are explicit

The embedded HUT JSON is authoritative for its named pages/usages, IDs, canonical names, type lists, and generated-page definitions. Its current schema does not encode every page-summary range, external page, PDF display alias, or prose deprecation marker. Those facts are retained as small reviewable CSV supplements rather than inferred and silently discarded.

This layout is designed for future schema evolution: the generator fails on unknown source kinds/types instead of guessing, while the runtime API already represents arbitrary numeric pages/usages and unknown future values.
