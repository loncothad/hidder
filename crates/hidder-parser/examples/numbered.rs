//! Parse a numbered Input report and print its layout.
#![allow(missing_docs)]

use hidder_parser::{
    ReportKind,
    parse,
};

fn main() {
    // Usage Page (Generic Desktop), Usage (Joystick), Collection (Application),
    // Report ID (1), Usage (X), Logical Min/Max, Report Size/Count, Input, End
    // Collection.
    let bytes = [
        0x05, 0x01, 0x09, 0x04, 0xA1, 0x01, 0x85, 0x01, 0x09, 0x30, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x01,
        0x81, 0x02, 0xC0,
    ];
    let descriptor = parse(&bytes).expect("Descriptor is well-formed");
    let report = descriptor.report(ReportKind::Input, 1).expect("Report ID 1");
    println!(
        "{:?} id={} {} bits / {} bytes / {} fields",
        report.kind,
        report.id,
        report.bit_len,
        report.byte_len(),
        report.field_count
    );
}
