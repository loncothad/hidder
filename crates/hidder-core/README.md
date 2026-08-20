# hidder-core

Allocation-free USB HID report descriptor item model, encoder, and streaming
parser. The crate is `no_std` and has no runtime dependencies.

Unknown and reserved tags are preserved instead of being dropped, so firmware
and host tools can round-trip descriptors that include vendor or future items.

`EncodedShort` stores at most five bytes inline. `Items` never allocates and
stops after the first truncated item so callers can report a single structural
error.

## Examples

[`examples/roundtrip.rs`](examples/roundtrip.rs) encodes a Usage Page short item
and parses it back through the streaming iterator:

```console
cargo run -p hidder-core --example roundtrip
```
