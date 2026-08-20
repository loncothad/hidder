# HID report descriptor specification coverage

The item model follows HID 1.11 section 6.2.2. The compile-time DSL, core encoder/parser, and semantic parser share the same tag and width definitions.

## Item wire formats

| Format | Generation | Parsing | Forward compatibility |
|---|---:|---:|---|
| Short item, 0-byte payload | Yes | Yes | Unknown type/tag preserved |
| Short item, 1-byte payload | Yes | Yes | Unknown type/tag preserved |
| Short item, 2-byte payload | Yes | Yes | Unknown type/tag preserved |
| Short item, 4-byte payload | Yes | Yes | Unknown type/tag preserved |
| Long item, 0–255-byte payload | Yes | Yes | Arbitrary tag/payload preserved |
| Truncation diagnostics with byte offsets | N/A | Yes | Partial model through `parse_lossy` |

The encoder rejects `0xFE` as a short-item prefix because HID reserves it for the long-item format.

## Main items

| HID item | DSL directive | Width checked | Semantic handling |
|---|---|---:|---|
| Input | `Input (...)` | 1, 2, or 4 | field, flags, bit offset, report layout |
| Output | `Output (...)` | 1, 2, or 4 | field, flags, bit offset, report layout |
| Feature | `Feature (...)` | 1, 2, or 4 | field, flags, bit offset, report layout |
| Collection | `Collection (...)` | exactly 1 | type, usage, parent, depth, source offsets |
| End Collection | `End Collection` | exactly 0 | stack validation and closing offset |
| Reserved/future Main tag | `Raw Short (...)` | 0, 1, 2, or 4 | preserved; Local state consumed |

Main flag categories retain all standard bits and unknown future bits. The DSL accepts named flags or `bits=0x...`; exact `size=` can force the legal 1/2/4-byte encoding.

## Global items

| HID item | DSL directive | Legal width(s) | Semantic state |
|---|---|---:|---|
| Usage Page | `Usage Page (...)` | 1, 2 | 16-bit page |
| Logical Minimum | `Logical Minimum (...)` | 1, 2, 4 | signed |
| Logical Maximum | `Logical Maximum (...)` | 1, 2, 4 | signed iff minimum is negative, otherwise unsigned |
| Physical Minimum | `Physical Minimum (...)` | 1, 2, 4 | signed |
| Physical Maximum | `Physical Maximum (...)` | 1, 2, 4 | signed iff minimum is negative, otherwise unsigned |
| Unit Exponent | `Unit Exponent (...)` | 1 | signed low nibble |
| Unit | `Unit (...)` | 1, 2, 4 | raw packed unit and structured unit syntax |
| Report Size | `Report Size (...)` | 1, 2, 4 | checked 1..=32 for fields |
| Report ID | `Report ID (...)` | 1 | checked 1..=255 |
| Report Count | `Report Count (...)` | 1, 2, 4 | checked non-zero for fields |
| Push | `Push` | 0 | complete Global-state stack |
| Pop | `Pop` | 0 | underflow/unbalanced-stack diagnostics |
| Reserved/future Global tag | `Raw Short (...)` | 0, 1, 2, 4 | preserved |

## Local items

| HID item | DSL directive | Legal width(s) | Semantic handling |
|---|---|---:|---|
| Usage | `Usage (...)` | 1, 2, 4 | short or extended 32-bit usage |
| Usage Minimum | `Usage Minimum (...)` | 1, 2, 4 | page/range validation |
| Usage Maximum | `Usage Maximum (...)` | 1, 2, 4 | page/range validation |
| Designator Index | `Designator Index (...)` | 1, 2, 4 | explicit index list |
| Designator Minimum | `Designator Minimum (...)` | 1, 2, 4 | range validation |
| Designator Maximum | `Designator Maximum (...)` | 1, 2, 4 | range validation |
| String Index | `String Index (...)` | 1, 2, 4 | explicit index list |
| String Minimum | `String Minimum (...)` | 1, 2, 4 | range validation |
| String Maximum | `String Maximum (...)` | 1, 2, 4 | range validation |
| Delimiter | `Delimiter (Open/Close)` | exactly 1 | alternate Usage sets; nesting rejected |
| Reserved/future Local tag | `Raw Short (...)` | 0, 1, 2, 4 | preserved |

Local state is cleared after every Main item, including unknown future Main tags.

## Usage Tables model

The generated HUT 1.7 catalog includes:

- 2,770 individually named usage records from the PDF's JSON attachment;
- all source numeric page and usage IDs;
- all 17 observed Usage Type classifications as multi-valued bitsets;
- three generated full 16-bit pages (Button, Ordinal, Monitor Enumerated);
- explicit Undefined, Unicode, and Gaming Device page records from the HUT page summary;
- reserved and vendor-defined page ranges from HUT table 3.1;
- complement ranges for unassigned IDs within finite defined pages;
- source/search aliases retained separately from canonical names; and
- explicit deprecation status supplements where HUT 1.7 says not to use an entry.

Complement ranges are generated from the finite named assignments in the current source revision. They describe IDs unassigned by that revision; a future HUT update can turn such an ID into a defined record without an API representation change.

## Checks deliberately left configurable

`hidder_dsl::CompileOptions` can disable structural, report-shape, or usage-type linting for tooling that must encode intentionally incomplete fragments. Byte-level width/range safety is still enforced. `hidder_parser::ParseOptions { strict: false }` retains the semantic model while relaxing specification diagnostics.
