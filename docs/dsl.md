# Descriptor text language

The language is line-oriented. One non-empty line contains one directive. `#` starts a whole-line comment after indentation. `//` starts a trailing comment outside quoted text. Directive names, named metadata, and flag words are matched case-insensitively while ignoring spaces, `_`, `-`, `/`, and punctuation used as separators.

```text
Directive Name (positional argument, key=value, ...)
Push
End Collection
```

Values accept decimal, hexadecimal (`0x`), octal (`0o`), or binary (`0b`) notation with `_` separators. Signed values may begin with `+` or `-`.

## Main items

```text
Input (Data, Variable, Absolute)
Output (Data, Variable, Absolute, NonVolatile)
Feature (Data, Variable, Absolute, size=2)
Collection (Application)
End Collection
```

Collection names:

- `Physical`, `Application`, `Logical`, `Report`, `Named Array`, `Usage Switch`, `Usage Modifier`;
- a numeric byte for reserved/vendor values.

Main item flags are grouped by HID bit position. At most one value from each group may be supplied:

| Bit/group | Clear value | Set value |
|---|---|---|
| 0 | `Data` | `Constant` |
| 1 | `Array` | `Variable` |
| 2 | `Absolute` | `Relative` |
| 3 | `No Wrap` | `Wrap` |
| 4 | `Linear` | `Nonlinear` |
| 5 | `Preferred State` | `No Preferred` |
| 6 | `No Null Position` | `Null State` |
| 7 | `NonVolatile` | `Volatile` (Output/Feature) |
| 8 | `Bit Field` | `Buffered Bytes` |

`Input`, `Output`, and `Feature` also accept `bits=0x...` to retain future flag bits. `size=1|2|4` forces the payload width; otherwise the smallest legal width is selected.

## Global items

```text
Usage Page (Generic Desktop)
Usage Page (0xff00, size=2)
Logical Minimum (-127)
Logical Maximum (127)
Physical Minimum (-1000, size=2)
Physical Maximum (1000, size=2)
Unit Exponent (-2)
Unit (SI Linear, length=1, time=-1)
Unit (0x0011, size=2)
Report Size (8)
Report ID (1)
Report Count (3)
Push
Pop
```

Named unit systems are `None`, `SI Linear`, `SI Rotation`, `English Linear`, and `English Rotation`. Structured `Unit` accepts signed four-bit exponents `length`, `mass`, `time`, `temperature`, `current`, and `luminous_intensity` (separator-insensitive). A numeric Unit preserves arbitrary/future nibbles.

`Push` and `Pop` operate on the complete Global state. The compiler rejects Pop underflow and unbalanced Push at end of input.

## Local items

```text
Usage (Mouse)
Usage (0x0030)
Usage (Generic Desktop:X)
Usage (0x00010030, size=4)
Usage Minimum (Button 1)
Usage Maximum (Button 8)
Designator Index (1)
Designator Minimum (1)
Designator Maximum (4)
String Index (2)
String Minimum (2)
String Maximum (5)
Delimiter (Open)
Usage (X)
Delimiter (Close)
```

A named Usage is resolved in the active Usage Page. Generated names such as `Button 42`, `Instance 7`, and `Enum 3` are accepted. Canonical names, slugs, and recorded aliases are searchable.

An extended usage can be written as `Page:Usage`, where either side is a name or number. It is emitted as a four-byte local item and does not require changing the active Usage Page. A full numeric value greater than `0xffff` is interpreted as the 32-bit `page << 16 | id` form.

The compiler validates that Usage Minimum/Maximum share a page and are ordered. Designator and String ranges must also be ordered. Delimiter sets cannot nest and must close before a Main item.

## Exact widths

Any numeric standard item that allows multiple widths accepts `size=1`, `size=2`, or `size=4` as permitted by HID 1.11. The value must fit exactly. Fixed-width items either reject `size=` or accept only their mandated width:

- Usage Page: 1 or 2;
- Unit Exponent, Report ID, Delimiter, Collection: 1;
- Push, Pop, End Collection: 0 (no arguments);
- all other numeric standard items: 1, 2, or 4;
- Input/Output/Feature: 1, 2, or 4.

## Raw/future forms

```text
Raw Short (type=Main, tag=0, data=[])
Raw Short (type=Global, tag=15, data=[0x12, 0x34, 0x56, 0x78])
Raw Short (type=Local, tag=14, data=[1])
Raw Short (type=Reserved, tag=14, data=[0x34, 0x12])

Long Item (tag=0xaa, data=[0x10, 0x20, 0x30])
Raw Bytes ([0xde, 0xad, 0xbe, 0xef])
```

`Raw Short` permits Main, Global, Local, or Reserved type numbers and any four-bit tag. Payload length must be 0, 1, 2, or 4. A combination whose header would be `0xFE` is rejected because the wire format would be decoded as a long item.

`Long Item` permits a tag byte and 0–255 payload bytes. `Raw Bytes` accepts any non-empty byte sequence, does not update compiler state, and should be reserved for malformed-descriptor tests or wire encodings not representable as HID items.

## Compile-time diagnostics

The procedural macros turn compiler errors into Rust `compile_error!` invocations. Checks include:

- recognized directives and arguments;
- duplicate/unknown named arguments;
- numeric syntax, range, signedness, and exact-width fit;
- standard item legal widths;
- resolved page/usage names;
- Usage Page and Usage 16-bit limits;
- Report ID 1..=255;
- Report Size 1..=32 and non-zero Report Count before fields;
- Logical, Usage, Designator, and String range ordering;
- Collection/End Collection, Push/Pop, and Delimiter balance; and
- collection Usage Type compatibility when catalog metadata is available.

The reusable `hidder_dsl::compile` API returns non-fatal warnings separately. Warnings identify reserved/unknown page or usage values, deprecated usages, collection usage-type mismatches, and `Raw Bytes`.
