//! Compile a joystick descriptor at runtime (the same compiler the macro uses).
#![allow(missing_docs)]

use hidder_dsl::compile;

const SOURCE: &str = r"
    Usage Page (Generic Desktop)
    Usage (Joystick)
    Collection (Application)
      Usage (Pointer)
      Collection (Physical)
        Usage (X)
        Usage (Y)
        Logical Minimum (-127)
        Logical Maximum (127)
        Report Size (8)
        Report Count (2)
        Input (Data, Variable, Absolute)

        Usage (Hat Switch)
        Logical Minimum (0)
        Logical Maximum (7)
        Report Size (4)
        Report Count (1)
        Input (Data, Variable, Absolute, Null State)
        Report Count (1)
        Report Size (4)
        Input (Constant, Array, Absolute)
      End Collection
    End Collection
";

fn main() {
    let compiled = compile(SOURCE).unwrap_or_else(|errors| {
        for error in errors {
            eprintln!("{error}");
        }
        std::process::exit(1);
    });
    println!(
        "{} bytes, {} items, {} warnings",
        compiled.bytes.len(),
        compiled.item_count,
        compiled.warnings.len()
    );
}
