//! Encode a boot-protocol mouse descriptor and print the item stream.
#![allow(missing_docs)]

use hidder::{
    Items,
    hid_report_descriptor,
};

hid_report_descriptor! {
    pub static MOUSE_REPORT_DESCRIPTOR = r#"
        Usage Page (Generic Desktop)
        Usage (Mouse)
        Collection (Application)
          Usage (Pointer)
          Collection (Physical)
            Usage Page (Button)
            Usage Minimum (Button 1)
            Usage Maximum (Button 3)
            Logical Minimum (0)
            Logical Maximum (1)
            Report Count (3)
            Report Size (1)
            Input (Data, Variable, Absolute)
            Report Count (1)
            Report Size (5)
            Input (Constant, Array, Absolute)
            Usage Page (Generic Desktop)
            Usage (X)
            Usage (Y)
            Logical Minimum (-127)
            Logical Maximum (127)
            Report Size (8)
            Report Count (2)
            Input (Data, Variable, Relative)
          End Collection
        End Collection
    "#;
}

fn main() {
    println!("{} descriptor bytes", MOUSE_REPORT_DESCRIPTOR.len());
    for item in Items::new(&MOUSE_REPORT_DESCRIPTOR) {
        println!("{item:?}");
    }
}
