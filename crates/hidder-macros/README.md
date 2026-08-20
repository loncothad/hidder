# hidder-macros

Compile-time macros that turn a HID-tool-style descriptor into a fixed
`[u8; N]`. Prefer the [`hidder`](../hidder) facade unless you need to
depend on the proc-macro crate directly.

The generated value is an ordinary constant: no allocation, initialization, or
runtime parsing. Invalid item widths, unresolved names, overflowing values, and
unbalanced collections become compile errors at the source line.

## Examples

[`examples/consumer.rs`](examples/consumer.rs) builds a Consumer Control volume
descriptor as a `const [u8; N]`:

```console
cargo run -p hidder-macros --example consumer
```
