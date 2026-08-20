# hid-usage-tables

`no_std` numeric catalog of **USB HID Usage Tables 1.7**. Pages, usages, generated
ranges, reserved ranges, aliases, and deprecation status are retained as data
rather than collapsed into closed Rust enums.

Unknown numeric identifiers stay representable through
[`hidder_core::Usage`].

Regenerate the checked-in catalog from the workspace root with
`cargo xtask generate`.

## Examples

[`examples/classify.rs`](examples/classify.rs) resolves a named page, a defined
usage, a generated Button ID, a reserved ID, and a vendor-defined page:

```console
cargo run -p hid-usage-tables --example classify
```
