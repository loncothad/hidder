# Release validation

This file records the checks applied to the `0.1.0` source archive.

## Specification provenance

- The checked-in `spec/upstream/HidUsageTables.json` was extracted from the PDF attachment in USB-IF **HID Usage Tables 1.7**.
- The extracted attachment and the checked-in file were compared byte-for-byte.
- SHA-256: `07e8ca728d78271cb0186482e51479deee32f1ca34016473623173638a2144d8`.
- The source declares HUT version `1.7.0` and contains 33 source pages and 2,770 individually named usages.

## Independent catalog checks

A generator-independent release script parsed the source JSON and generated Rust data, then verified:

- every source `(page_id, usage_id, name, usage_types)` tuple appears exactly once;
- all generated slugs, aliases, status records, page slices, and usage slices are internally consistent;
- 330 per-page unassigned/reserved complement ranges exactly cover every unnamed ID on finite defined pages;
- named pages plus page-range records form a non-overlapping, gap-free partition of `0x0000..=0xffff`;
- all eight Cargo manifests parse and every workspace/path dependency resolves;
- all Markdown relative links resolve;
- all Rust source files pass delimiter, string/comment, UTF-8, placeholder, NUL, and trailing-whitespace scans.

The resulting checked-in catalog contains 36 page records, 2,770 named usage records, 7 aliases, 330 usage ranges, and 11 page ranges.

## Build and test commands

The workspace now requires Rust `1.100` and temporarily selects the `beta`
toolchain until `1.100.0` is released. The compiler-backed development checks
include:

```console
cargo test --workspace --all-features
cargo xtask verify
cargo check -p hidder-core --target thumbv7em-none-eabihf
cargo check -p hid-usage-tables --target thumbv7em-none-eabihf
cargo check -p hidder-dsl --target thumbv7em-none-eabihf
cargo check -p hidder-parser --target thumbv7em-none-eabihf
cargo check -p hidder --no-default-features --target thumbv7em-none-eabihf
cargo doc --workspace --all-features --no-deps
```

The original archive-packaging environment did not contain a Rust toolchain and could not reach a Rust distribution server, so those Cargo commands were not executed during packaging. The source/catalog/provenance and archive-integrity checks above were executed locally. Run `just ci` for the current compiler-backed validation gate; formatting requires nightly rustfmt as described in the workspace README.
