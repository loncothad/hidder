# hid-report-toolkit

A dependency-free Rust workspace for generating, inspecting, and parsing USB HID report descriptors.

The descriptor source is human-readable text compiled by a procedural macro into a fixed `[u8; N]`. The emitted value is an ordinary constant: it performs no allocation, initialization, or runtime parsing and can be used by `#![no_std]` firmware. The same byte/item model is used by an allocation-free streaming parser and an optional `no_std + alloc` semantic parser.

The checked-in usage catalog is generated from the machine-readable attachment embedded in **USB HID Usage Tables 1.7**, supplemented with metadata that the attachment does not itself encode. It preserves numeric identifiers, all usage-type classifications, generated ranges, reserved/unassigned ranges, page classes, aliases, and deprecation status rather than reducing the tables to Rust enums.

## Workspace layout

| Package | Runtime profile | Purpose |
|---|---|---|
| `hidder` | `no_std`; parser feature uses `alloc` | Public facade and macro re-exports |
| `hidder-core` | `no_std`, no allocation | Short/long item model, encoder, and streaming parser |
| `hid-usage-tables` | `no_std`, no allocation | Generated HUT 1.7 numeric metadata catalog |
| `hidder-dsl` | `no_std + alloc` | Shared textual compiler and semantic validation |
| `hidder-macros` | host `std` proc macro | Compile-time text-to-array macros |
| `hidder-parser` | `no_std + alloc` | Full semantic report-descriptor parser |
| `xtask` | host `std` | Deterministic catalog generation and spec updates |

There are no third-party Rust crate dependencies. The workspace MSRV is Rust 1.100.

`rust-toolchain.toml` targets the stable Rust `1.100.0` release, which is not yet
available from rustup. Local validation used Rust 1.100 beta; beta is not pinned
or required by the repository. Package editions remain Rust 2021.

Rust 1.100 stabilizes the Allocator API, but this workspace has no raw allocation
calls to migrate. Its existing `alloc` containers and allocation-free firmware
APIs are unchanged; no custom-allocator parameters are added.

## Compile a descriptor

```rust
#![no_std]

use hidder::hid_report_descriptor;

hid_report_descriptor! {
    pub static MOUSE_REPORT_DESCRIPTOR = r#"
        Usage Page (Generic Desktop)
        Usage (Mouse)
        Collection (Application)
          Usage (Pointer)
          Collection (Physical)
            Usage Page (Button)
            Usage Minimum (Button 1)
            Usage Maximum (Button 3)
            Logical Minimum (0)
            Logical Maximum (1)
            Report Count (3)
            Report Size (1)
            Input (Data, Variable, Absolute)

            Report Count (1)
            Report Size (5)
            Input (Constant, Array, Absolute)

            Usage Page (Generic Desktop)
            Usage (X)
            Usage (Y)
            Logical Minimum (-127)
            Logical Maximum (127)
            Report Size (8)
            Report Count (2)
            Input (Data, Variable, Relative)
          End Collection
        End Collection
    "#;
}
```

This declaration has the inferred type `[u8; 50]`. Invalid item widths, unresolved names, overflowing values, malformed flags, unbalanced collections/global stacks/delimiters, invalid report IDs, invalid usage ranges, and incomplete report field shapes become compile errors at the source line.

Two expression macros are also available:

```rust
use hidder::{hid_report, hid_report_len};

const BYTES: [u8; 7] = hid_report!(r#"
    Usage Page (Generic Desktop)
    Usage (Mouse)
    Collection (Application)
    End Collection
"#);

const BYTE_COUNT: usize = hid_report_len!(r#"
    Usage Page (Generic Desktop)
    Usage (Mouse)
    Collection (Application)
    End Collection
"#);
```

Names are case- and separator-insensitive. Numeric pages/usages remain available for vendor-defined and newly published assignments. Four-byte local usages can be written as `Usage (page:id)` or as a full `0xPPPPUUUU` value.

## Custom and future items

The standard DSL covers every Main, Global, and Local item in HID 1.11. Escape hatches preserve forward compatibility:

```text
Raw Short (type=Reserved, tag=14, data=[0x34, 0x12])
Long Item (tag=0xaa, data=[0x10, 0x20, 0x30])
Raw Bytes ([0xde, 0xad, 0xbe, 0xef])
```

`Raw Short` still enforces the HID short-item structure and rejects the reserved `0xFE` long-item prefix collision. `Long Item` accepts any tag and up to 255 payload bytes. `Raw Bytes` intentionally bypasses item and state validation and produces a warning in the reusable DSL compiler.

## Parse descriptors

The allocation-free item stream is always available:

```rust
use hidder::{Item, Items};

for item in Items::new(&MOUSE_REPORT_DESCRIPTOR) {
    match item?.item {
        Item::Short(short) => { /* exact type/tag/width/payload */ }
        Item::Long(long) => { /* tag plus borrowed payload */ }
    }
}
# Ok::<(), hidder::core::ParseError>(())
```

Enable the `parser` feature for the semantic model:

```toml
[dependencies]
hidder = { path = "crates/hidder", features = ["parser"] }
```

```rust
use hidder::parser::{parse, ReportKind};

let descriptor = parse(&MOUSE_REPORT_DESCRIPTOR).unwrap();
let input = descriptor.report(ReportKind::Input, 0).unwrap();
assert_eq!(input.byte_len(), 3);
```

The semantic parser resolves Global and Local state, Push/Pop, delimiter usage sets, collections, report IDs, per-report bit offsets, report sizes, units, strings, designators, and usage ranges. Long items and unknown/reserved short items are retained verbatim. `parse_lossy` returns a partial model plus diagnostics for malformed input.

## Metadata-rich usage tables

```rust
use hidder::{lookup_usage, Usage};
use hidder::usages::{UsageLookup, UsageType};

let UsageLookup::Defined(entry) = lookup_usage(Usage::new(0x0001, 0x0030)) else {
    unreachable!();
};
assert_eq!(entry.name, "X");
assert!(entry.types.contains(UsageType::DynamicValue));

let generated = lookup_usage(Usage::new(0x0009, 42)); // Button 42
let vendor = lookup_usage(Usage::new(0xff00, 1));
```

The API intentionally distinguishes:

- individually defined usages;
- algorithmically generated usages such as Button, Ordinal, and Monitor Enumerated;
- reserved/unassigned IDs inside known pages;
- undefined and externally specified pages;
- reserved page ranges;
- vendor-defined pages; and
- identifiers unknown to the current source revision.

Every `UsageEntry` retains the page/ID, canonical source name, generated slug, the complete multi-valued `UsageTypeSet`, status, and aliases. `Usage` remains a transparent 32-bit numeric identifier, so unknown and future values are never made unrepresentable.

## Regenerate or update the catalog

```console
cargo xtask verify
cargo xtask stats
cargo xtask generate --check
cargo xtask generate
```

To update from a newer USB-IF HUT PDF containing the machine-readable attachment:

```console
cargo xtask update path/to/hut.pdf \
  --publication-date YYYY-MM-DD \
  --url https://www.usb.org/path/to/new-hut.pdf
```

`update` uses Poppler's `pdfdetach`, verifies the JSON schema/version, computes SHA-256, rewrites the provenance manifest, regenerates deterministically, and rolls all changed files back if any step fails. Review and update the small CSV supplements because reserved page ranges, aliases, external pages, and deprecation annotations are not all represented in the embedded JSON.

## Development commands

Builds, tests, Clippy, catalog tooling, and embedded checks target stable Rust
1.100.0. Development recipes use ordinary `cargo` commands without forcing beta.

Formatting still requires nightly rustfmt (and Taplo for TOML): the existing
`.rustfmt.toml` uses unstable options. Install nightly rustfmt with
`rustup toolchain install nightly --profile minimal --component rustfmt`, then
run `RUSTUP_TOOLCHAIN=nightly just fmt` or `RUSTUP_TOOLCHAIN=nightly just fmt-check`
for formatting only. This formatter-only exception does not require nightly
to build the workspace.

```console
just ci
just fmt
just clippy
just test
just verify
just embedded
just doc
just generate-check
```

See [`docs/dsl.md`](docs/dsl.md), [`docs/parser.md`](docs/parser.md), [`docs/codegen.md`](docs/codegen.md), [`SPEC_COVERAGE.md`](SPEC_COVERAGE.md), and [`VALIDATION.md`](VALIDATION.md) for the complete reference and release checks.

## Specification provenance

- Device Class Definition for Human Interface Devices, version 1.11.
- HID Usage Tables, version 1.7, published 2026-01-27.
- Checked-in `HidUsageTables.json` SHA-256: `07e8ca728d78271cb0186482e51479deee32f1ca34016473623173638a2144d8`.

USB-IF specification names and usage names are used for interoperability and identification. The workspace source code is dual-licensed under MIT or Apache-2.0; the cited USB specifications remain subject to their publishers' terms.
