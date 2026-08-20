# hidder

Facade crate for USB HID report descriptor generation, usage-table metadata,
and optional semantic parsing.

The default `macros` feature exposes compile-time generation without adding
runtime allocation. The optional `parser` feature exposes the `no_std + alloc`
semantic parser.

See the [workspace README](../../README.md), [DSL reference](../../docs/dsl.md),
and [parser guide](../../docs/parser.md) for the complete language and coverage
notes.

## Examples

[`examples/mouse.rs`](examples/mouse.rs) compiles a boot-protocol mouse
descriptor and dumps the allocation-free item stream:

```console
cargo run -p hidder --example mouse
```

[`examples/parse.rs`](examples/parse.rs) compiles a keyboard descriptor and
prints semantic report layouts (`parser` feature):

```console
cargo run -p hidder --example parse --features parser
```
