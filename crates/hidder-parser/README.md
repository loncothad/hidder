# hidder-parser

Full `no_std + alloc` semantic parser for USB HID report descriptors. It builds
collections, report layouts, usage sets, and Global state snapshots while
retaining long and unknown items.

For the allocation-free item stream, use `hidder-core::Items` instead.
`parse` fails when any error-level diagnostic is present. `parse_lossy` always
returns a partial model plus diagnostics so tools can keep going on malformed
input.

Parse records are stored in any [`RecordBuf`]. [`parse`] uses `Vec`; [`parse_in`]
accepts `VecDeque`, `LinkedList`, or a custom [`DescriptorStore`].

## Examples

[`examples/numbered.rs`](examples/numbered.rs) parses a joystick Input report
that uses Report ID 1:

```console
cargo run -p hidder-parser --example numbered
```

[`examples/stores.rs`](examples/stores.rs) fills `Vec`, `VecDeque`, and
`LinkedList` from the same bytes:

```console
cargo run -p hidder-parser --example stores
```
