//! Compile a Consumer Control volume descriptor into a `[u8; N]` constant.
#![allow(missing_docs)]

use hidder_macros::hid_report_descriptor;

hid_report_descriptor! {
    const VOLUME = r#"
        Usage Page (Consumer)
        Usage (Consumer Control)
        Collection (Application)
          Usage (Volume Increment)
          Usage (Volume Decrement)
          Usage (Mute)
          Logical Minimum (0)
          Logical Maximum (1)
          Report Size (1)
          Report Count (3)
          Input (Data, Variable, Absolute)
          Report Count (1)
          Report Size (5)
          Input (Constant, Array, Absolute)
        End Collection
    "#;
}

fn main() {
    println!("{} bytes: {:02x?}", VOLUME.len(), VOLUME);
}
