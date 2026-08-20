//! Encode a short Usage Page item and parse it back without allocating.
#![allow(missing_docs)]

use hidder_core::{
    EncodeWidth,
    GlobalTag,
    Item,
    ItemTag,
    ItemType,
    Items,
    encode_unsigned,
};

fn main() {
    let encoded = encode_unsigned(ItemType::Global, GlobalTag::UsagePage as u8, 0x01, EncodeWidth::Auto)
        .expect("0X01 fits a one-byte unsigned item");

    let spanned = Items::new(encoded.as_slice())
        .next()
        .expect("One item")
        .expect("Complete item");

    match spanned.item {
        | Item::Short(short) => {
            assert_eq!(short.tag(), ItemTag::Global(GlobalTag::UsagePage));
            assert_eq!(short.unsigned_value(), 0x01);
            println!("Prefix={:#04x} value={}", short.prefix(), short.unsigned_value());
        },
        | Item::Long(_) => unreachable!("Unsigned encoder emits short items"),
    }
}
