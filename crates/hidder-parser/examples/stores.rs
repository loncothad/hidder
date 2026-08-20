//! Parse the same descriptor into `Vec`, `VecDeque`, and `LinkedList` stores.
#![allow(missing_docs)]

use hidder_parser::{
    LinkedListStore,
    ReportKind,
    VecDequeStore,
    parse,
    parse_in,
};

fn main() {
    let bytes = [
        0x05, 0x01, 0x09, 0x04, 0xA1, 0x01, 0x85, 0x01, 0x09, 0x30, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x01,
        0x81, 0x02, 0xC0,
    ];

    let vec = parse(&bytes).unwrap();
    let deque = parse_in::<VecDequeStore>(&bytes).unwrap();
    let list = parse_in::<LinkedListStore>(&bytes).unwrap();

    for (label, bit_len, fields) in [
        (
            "Vec",
            vec.report(ReportKind::Input, 1).unwrap().bit_len,
            vec.fields.len(),
        ),
        (
            "VecDeque",
            deque.report(ReportKind::Input, 1).unwrap().bit_len,
            deque.fields.len(),
        ),
        (
            "LinkedList",
            list.report(ReportKind::Input, 1).unwrap().bit_len,
            list.fields.len(),
        ),
    ] {
        println!("{label}: {fields} field(s), {bit_len} bits");
    }
}
