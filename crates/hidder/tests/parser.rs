#![cfg(feature = "parser")]
#![allow(missing_docs)]

use hidder::{
    Usage,
    hid_report_descriptor,
    parser,
    usages,
};

hid_report_descriptor! {
    const MOUSE = r#"
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

#[test]
fn macro_parser_and_catalog_interoperate() {
    let descriptor = parser::parse(&MOUSE).unwrap();
    let report = descriptor.report(parser::ReportKind::Input, 0).unwrap();
    assert_eq!(report.byte_len(), 3);
    assert_eq!(descriptor.collections.len(), 2);
    assert_eq!(descriptor.fields.len(), 3);

    let usages::UsageLookup::Defined(x) = descriptor.fields[2].primary_usage_metadata().expect("X usage metadata")
    else {
        panic!("Expected a defined HUT record");
    };
    assert_eq!(x.usage(), Usage::new(0x0001, 0x0030));
    assert_eq!(x.name, "X");
}
