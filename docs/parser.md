# Parser architecture and API

Two parsers share `hidder-core`'s item definitions.

## Allocation-free item stream

`hidder_core::Items<'a>` is an iterator over `Result<SpannedItem<'a>, ParseError>`. It borrows the original bytes and performs no allocation.

A short item exposes:

- raw prefix, type, four-bit tag, and exact `DataSize`;
- borrowed payload bytes;
- unsigned and sign-extended numeric interpretations; and
- a known `MainTag`, `GlobalTag`, `LocalTag`, or lossless unknown type/tag.

A long item exposes its tag and borrowed payload. Truncated short or long items report the starting byte offset, total bytes needed, and bytes available. The iterator stops after truncation because no unambiguous next item exists.

## Semantic parser

Enable the facade's `parser` feature or depend directly on `hidder-parser`. It is `#![no_std]` and uses `alloc` for the model.

```rust
use hidder_parser::{parse, parse_in, ReportKind, VecDequeStore};

let descriptor = parse(bytes)?;
let input = descriptor.report(ReportKind::Input, 1);
let queued = parse_in::<VecDequeStore>(bytes)?;
# let _ = (input, queued);
# Ok::<(), hidder_parser::ParseFailure<_, _>>(())
```

`Descriptor` contains:

- `fields`: Input, Output, and Feature fields in source order;
- `collections`: a pre-order collection tree with parent/depth and source offsets;
- `reports`: independent `(kind, report_id)` bit/byte layouts;
- `long_items`: all borrowed long-item records; and
- `unknown_items`: all reserved/future short-item records.

Each `ReportField` is a snapshot of all relevant Global and Local state: report ID, bit offset, size/count, complete Main flags, logical/physical bounds, Unit and Unit Exponent, usage sets/range, designators, strings, and containing collection. `primary_usage_metadata()` resolves directly through the generated HUT catalog.

## Report layout rules

Input, Output, and Feature have separate report namespaces. Each Report ID has its own bit cursor. A numbered report begins at bit offset 8 to account for the transmitted ID byte. Unnumbered reports begin at zero. `ReportLayout::byte_len()` rounds the final bit length upward.

Strict mode diagnoses mixing numbered and unnumbered fields in one descriptor, zero sizes/counts, size values above the HID 1.11 32-bit field limit, range inversions, illegal item widths, and report-length overflow.

## Global and Local state

The parser implements the complete Global state stack. `Push` snapshots all Global items and `Pop` restores them. Underflow and unterminated stacks are diagnosed.

Local state is consumed and reset by every Main item. This includes unknown future Main tags, which prevents an unrecognized Main item from accidentally leaking Usage assignments into the next known field. Delimiters create alternate Usage sets and cannot nest.

For Logical/Physical Maximum, the parser follows HID signedness rules: the maximum is decoded as signed when its corresponding minimum is negative and unsigned otherwise.

## Strict and lossy modes

- `parse(bytes)` and `parse_with_options(...)` store records in `Vec` and return `Err(Vec<Diagnostic>)` when any error is present.
- `parse_in::<S>(bytes)` stores records in any [`DescriptorStore`] (including `VecDequeStore` and `LinkedListStore`).
- `parse_lossy(bytes)` and `parse_lossy_with_options(...)` always return a partial `Descriptor` plus every diagnostic accumulated before fatal truncation.
- `ParseOptions { strict: false }` retains structural interpretation while suppressing most specification-conformance diagnostics.

Warnings and errors carry descriptor byte offsets. Unknown/reserved items are not errors by themselves; they are preserved for forward compatibility.
