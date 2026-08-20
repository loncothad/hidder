# hidder-dsl

Shared `no_std + alloc` compiler for the textual USB HID report descriptor
language used by `hidder-macros`. Firmware does not depend on this crate at
runtime: the output is a `Vec<u8>` that can be written to a constant array.

Names are case- and separator-insensitive. Numeric pages and usages remain
available for vendor-defined assignments. See [`docs/dsl.md`](../../docs/dsl.md)
in the workspace for the full language.

## Examples

[`examples/joystick.rs`](examples/joystick.rs) compiles a joystick with a hat
switch and prints the encoded size:

```console
cargo run -p hidder-dsl --example joystick
```
