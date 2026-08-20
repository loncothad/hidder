//! Classify defined, generated, reserved, and vendor-defined usages.
#![allow(missing_docs)]

use hid_usage_tables::{
    UsageLookup,
    UsageType,
    find_page,
    find_usage,
    lookup_usage,
};
use hidder_core::Usage;

fn main() {
    let page = find_page("generic desktop").expect("HUT names Generic Desktop");
    println!("Page {} is {}", page.id, page.name);

    let x = find_usage(page.id, "X").expect("Axis X");
    let UsageLookup::Defined(entry) = lookup_usage(x) else {
        panic!("X is individually defined");
    };
    println!(
        "{} types contain DV: {}",
        entry.name,
        entry.types.contains(UsageType::DynamicValue)
    );

    for usage in [Usage::new(0x0009, 42), Usage::new(0x0001, 0), Usage::new(0xFF00, 1)] {
        println!("{usage:?} => {:?}", lookup_usage(usage));
    }
}
