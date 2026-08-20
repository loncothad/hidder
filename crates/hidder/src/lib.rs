#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! `no_std` facade for USB HID report descriptors.
//!
//! The default `macros` feature exposes compile-time generation without adding
//! runtime allocation or initialization. The optional `parser` feature exposes
//! the `no_std + alloc` semantic parser.
//!
//! ```
//! use hidder::{
//!     Items,
//!     hid_report_descriptor,
//! };
//!
//! hid_report_descriptor! {
//!     pub const MOUSE = r#"
//!         Usage Page (Generic Desktop)
//!         Usage (Mouse)
//!         Collection (Application)
//!           Usage (Pointer)
//!           Collection (Physical)
//!             Usage Page (Button)
//!             Usage Minimum (Button 1)
//!             Usage Maximum (Button 3)
//!             Logical Minimum (0)
//!             Logical Maximum (1)
//!             Report Count (3)
//!             Report Size (1)
//!             Input (Data, Variable, Absolute)
//!             Report Count (1)
//!             Report Size (5)
//!             Input (Constant, Array, Absolute)
//!             Usage Page (Generic Desktop)
//!             Usage (X)
//!             Usage (Y)
//!             Logical Minimum (-127)
//!             Logical Maximum (127)
//!             Report Size (8)
//!             Report Count (2)
//!             Input (Data, Variable, Relative)
//!           End Collection
//!         End Collection
//!     "#;
//! }
//!
//! # let count = Items::new(&MOUSE).count();
//! # assert!(count > 0);
//! ```

/// Allocation-free item model and streaming parser.
pub mod core {
    pub use hidder_core::*;
}

/// Numeric HID Usage Tables catalog and metadata.
pub mod usages {
    pub use hid_usage_tables::*;
}

#[cfg(feature = "parser")]
/// Semantic `no_std + alloc` parser.
pub mod parser {
    pub use hidder_parser::*;
}

pub use hid_usage_tables::{
    find_page,
    find_usage,
    lookup_page,
    lookup_usage,
};
pub use hidder_core::{
    CollectionType,
    DataSize,
    GlobalTag,
    Item,
    ItemTag,
    ItemType,
    Items,
    LocalTag,
    MainFlags,
    MainTag,
    Unit,
    UnitSystem,
    Usage,
};
#[cfg(feature = "macros")]
pub use hidder_macros::{
    hid_report,
    hid_report_descriptor,
    hid_report_len,
};
