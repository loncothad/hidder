#![allow(missing_docs)]

use hidder::{
    Item,
    ItemTag,
    Items,
    MainTag,
    hid_report,
    hid_report_descriptor,
    hid_report_len,
};

const SOURCE: &str = r"
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
";

hid_report_descriptor! {
    pub const MOUSE = r#"
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

const INLINE: [u8; 50] = hid_report!(
    r#"
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
"#
);

const ENCODED_LEN: usize = hid_report_len!(
    r#"
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
"#
);

#[test]
fn macro_forms_emit_the_reference_descriptor() {
    let expected = [
        0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x15, 0x00,
        0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0x05, 0x01, 0x09, 0x30,
        0x09, 0x31, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x02, 0x81, 0x06, 0xC0, 0xC0,
    ];
    assert_eq!(MOUSE, expected);
    assert_eq!(INLINE, expected);
    assert_eq!(ENCODED_LEN, expected.len());
    assert_eq!(SOURCE.lines().filter(|line| !line.trim().is_empty()).count(), 26);
}

#[test]
fn generated_bytes_use_the_shared_streaming_item_model() {
    let input_count = Items::new(&MOUSE)
        .map(Result::unwrap)
        .filter(|spanned| {
            matches!(
                spanned.item,
                Item::Short(item) if item.tag() == ItemTag::Main(MainTag::Input)
            )
        })
        .count();
    assert_eq!(input_count, 3);
}

#[test]
fn raw_escape_hatches_are_const_usable() {
    const CUSTOM: [u8; 10] = hid_report!(
        r#"
        Raw Short (type=Reserved, tag=15, data=[1, 2, 3, 4])
        Long Item (tag=0xaa, data=[0x10, 0x20])
    "#
    );
    assert_eq!(CUSTOM, [0xFF, 1, 2, 3, 4, 0xFE, 2, 0xAA, 0x10, 0x20]);
}
