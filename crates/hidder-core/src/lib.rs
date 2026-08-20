#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Allocation-free primitives shared by descriptor generation and parsing.
//!
//! This crate models every HID short-item type/tag combination, the long-item
//! escape form, standard Main item flags, collections, units, and 32-bit
//! extended usages. Unknown and reserved tags are intentionally preserved.
//!
//! ```
//! use hidder_core::{
//!     EncodeWidth,
//!     Item,
//!     ItemType,
//!     Items,
//!     encode_unsigned,
//! };
//!
//! let encoded = encode_unsigned(ItemType::Global, 0, 0x01, EncodeWidth::Auto).unwrap();
//! let parsed = Items::new(encoded.as_slice()).next().unwrap().unwrap();
//! assert!(matches!(parsed.item, Item::Short(_)));
//! ```

use core::fmt;

/// Maximum encoded length of a HID short item, including its prefix.
pub const MAX_SHORT_ITEM_LEN: usize = 5;

/// Prefix byte that introduces the HID long-item wire format.
pub const LONG_ITEM_PREFIX: u8 = 0xFE;

/// The two-bit HID short-item type field.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ItemType {
    /// Main items create fields or collections.
    Main     = 0,
    /// Global items alter persistent state.
    Global   = 1,
    /// Local items describe the next Main item.
    Local    = 2,
    /// Reserved by the HID specification.
    Reserved = 3,
}

impl ItemType {
    /// Decodes the two-bit representation used in a short-item prefix.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            | 0 => Self::Main,
            | 1 => Self::Global,
            | 2 => Self::Local,
            | _ => Self::Reserved,
        }
    }
}

/// A legal HID short-item payload size.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DataSize {
    /// No payload bytes.
    Zero,
    /// One payload byte.
    One,
    /// Two payload bytes.
    Two,
    /// Four payload bytes. The prefix size code is `0b11`.
    Four,
}

impl DataSize {
    /// Returns the number of payload bytes.
    #[must_use]
    pub const fn bytes(self) -> usize {
        match self {
            | Self::Zero => 0,
            | Self::One => 1,
            | Self::Two => 2,
            | Self::Four => 4,
        }
    }

    /// Returns the two-bit size code stored in a short-item prefix.
    #[must_use]
    pub const fn prefix_code(self) -> u8 {
        match self {
            | Self::Zero => 0,
            | Self::One => 1,
            | Self::Two => 2,
            | Self::Four => 3,
        }
    }

    /// Decodes a short-item prefix size code.
    #[must_use]
    pub const fn from_prefix_code(code: u8) -> Self {
        match code & 0x03 {
            | 0 => Self::Zero,
            | 1 => Self::One,
            | 2 => Self::Two,
            | _ => Self::Four,
        }
    }

    /// Converts a byte count into a legal short-item size.
    #[must_use]
    pub const fn from_byte_count(count: usize) -> Option<Self> {
        match count {
            | 0 => Some(Self::Zero),
            | 1 => Some(Self::One),
            | 2 => Some(Self::Two),
            | 4 => Some(Self::Four),
            | _ => None,
        }
    }
}

/// Standard Main-item tags from HID 1.11 section 6.2.2.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum MainTag {
    /// Input report field.
    Input         = 0x8,
    /// Output report field.
    Output        = 0x9,
    /// Begins a collection.
    Collection    = 0xA,
    /// Feature report field.
    Feature       = 0xB,
    /// Ends the innermost collection.
    EndCollection = 0xC,
}

impl MainTag {
    /// Resolves a raw four-bit Main tag.
    #[must_use]
    pub const fn from_raw(tag: u8) -> Option<Self> {
        match tag & 0x0F {
            | 0x8 => Some(Self::Input),
            | 0x9 => Some(Self::Output),
            | 0xA => Some(Self::Collection),
            | 0xB => Some(Self::Feature),
            | 0xC => Some(Self::EndCollection),
            | _ => None,
        }
    }
}

/// Standard Global-item tags from HID 1.11 section 6.2.2.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum GlobalTag {
    /// Selects a usage page.
    UsagePage       = 0x0,
    /// Logical minimum.
    LogicalMinimum  = 0x1,
    /// Logical maximum.
    LogicalMaximum  = 0x2,
    /// Physical minimum.
    PhysicalMinimum = 0x3,
    /// Physical maximum.
    PhysicalMaximum = 0x4,
    /// Unit exponent.
    UnitExponent    = 0x5,
    /// Encoded unit.
    Unit            = 0x6,
    /// Size in bits of one report field element.
    ReportSize      = 0x7,
    /// Non-zero report identifier.
    ReportId        = 0x8,
    /// Number of report field elements.
    ReportCount     = 0x9,
    /// Pushes the complete Global state.
    Push            = 0xA,
    /// Restores the last pushed Global state.
    Pop             = 0xB,
}

impl GlobalTag {
    /// Resolves a raw four-bit Global tag.
    #[must_use]
    pub const fn from_raw(tag: u8) -> Option<Self> {
        match tag & 0x0F {
            | 0x0 => Some(Self::UsagePage),
            | 0x1 => Some(Self::LogicalMinimum),
            | 0x2 => Some(Self::LogicalMaximum),
            | 0x3 => Some(Self::PhysicalMinimum),
            | 0x4 => Some(Self::PhysicalMaximum),
            | 0x5 => Some(Self::UnitExponent),
            | 0x6 => Some(Self::Unit),
            | 0x7 => Some(Self::ReportSize),
            | 0x8 => Some(Self::ReportId),
            | 0x9 => Some(Self::ReportCount),
            | 0xA => Some(Self::Push),
            | 0xB => Some(Self::Pop),
            | _ => None,
        }
    }
}

/// Standard Local-item tags from HID 1.11 section 6.2.2.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum LocalTag {
    /// One usage associated with the next Main item.
    Usage             = 0x0,
    /// First usage in a range.
    UsageMinimum      = 0x1,
    /// Last usage in a range.
    UsageMaximum      = 0x2,
    /// Designator index.
    DesignatorIndex   = 0x3,
    /// First designator in a range.
    DesignatorMinimum = 0x4,
    /// Last designator in a range.
    DesignatorMaximum = 0x5,
    /// String index.
    StringIndex       = 0x7,
    /// First string in a range.
    StringMinimum     = 0x8,
    /// Last string in a range.
    StringMaximum     = 0x9,
    /// Opens or closes an alternate usage set.
    Delimiter         = 0xA,
}

impl LocalTag {
    /// Resolves a raw four-bit Local tag.
    #[must_use]
    pub const fn from_raw(tag: u8) -> Option<Self> {
        match tag & 0x0F {
            | 0x0 => Some(Self::Usage),
            | 0x1 => Some(Self::UsageMinimum),
            | 0x2 => Some(Self::UsageMaximum),
            | 0x3 => Some(Self::DesignatorIndex),
            | 0x4 => Some(Self::DesignatorMinimum),
            | 0x5 => Some(Self::DesignatorMaximum),
            | 0x7 => Some(Self::StringIndex),
            | 0x8 => Some(Self::StringMinimum),
            | 0x9 => Some(Self::StringMaximum),
            | 0xA => Some(Self::Delimiter),
            | _ => None,
        }
    }
}

/// A decoded standard tag, or a raw reserved/custom tag.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ItemTag {
    /// A recognized Main tag.
    Main(MainTag),
    /// A recognized Global tag.
    Global(GlobalTag),
    /// A recognized Local tag.
    Local(LocalTag),
    /// A tag not assigned by HID 1.11, retained without interpretation.
    Unknown {
        /// Raw type bits.
        item_type: ItemType,
        /// Raw four-bit tag.
        tag:       u8,
    },
}

/// One decoded HID short item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShortItem<'a> {
    prefix:    u8,
    item_type: ItemType,
    tag:       u8,
    data:      &'a [u8],
}

impl<'a> ShortItem<'a> {
    /// Returns the original prefix byte.
    #[must_use]
    pub const fn prefix(self) -> u8 {
        self.prefix
    }

    /// Returns the short-item type.
    #[must_use]
    pub const fn item_type(self) -> ItemType {
        self.item_type
    }

    /// Returns the raw four-bit tag.
    #[must_use]
    pub const fn raw_tag(self) -> u8 {
        self.tag
    }

    /// Returns the payload size.
    #[must_use]
    pub const fn size(self) -> DataSize {
        DataSize::from_prefix_code(self.prefix)
    }

    /// Returns the payload bytes in descriptor byte order.
    #[must_use]
    pub const fn data(self) -> &'a [u8] {
        self.data
    }

    /// Classifies the item against the HID 1.11 standard tags.
    #[must_use]
    pub const fn tag(self) -> ItemTag {
        match self.item_type {
            | ItemType::Main => {
                match MainTag::from_raw(self.tag) {
                    | Some(tag) => ItemTag::Main(tag),
                    | None => {
                        ItemTag::Unknown {
                            item_type: ItemType::Main,
                            tag:       self.tag,
                        }
                    },
                }
            },
            | ItemType::Global => {
                match GlobalTag::from_raw(self.tag) {
                    | Some(tag) => ItemTag::Global(tag),
                    | None => {
                        ItemTag::Unknown {
                            item_type: ItemType::Global,
                            tag:       self.tag,
                        }
                    },
                }
            },
            | ItemType::Local => {
                match LocalTag::from_raw(self.tag) {
                    | Some(tag) => ItemTag::Local(tag),
                    | None => {
                        ItemTag::Unknown {
                            item_type: ItemType::Local,
                            tag:       self.tag,
                        }
                    },
                }
            },
            | ItemType::Reserved => {
                ItemTag::Unknown {
                    item_type: ItemType::Reserved,
                    tag:       self.tag,
                }
            },
        }
    }

    /// Interprets the payload as an unsigned little-endian value.
    #[must_use]
    pub fn unsigned_value(self) -> u32 {
        let mut value = 0_u32;
        let mut index = 0_usize;
        while index < self.data.len() {
            value |= u32::from(self.data[index]) << (index * 8);
            index += 1;
        }
        value
    }

    /// Interprets the payload as a signed two's-complement value.
    #[must_use]
    pub fn signed_value(self) -> i32 {
        let unsigned = self.unsigned_value();
        match self.data.len() {
            | 0 => 0,
            | 1 => i32::from(unsigned as u8 as i8),
            | 2 => i32::from(unsigned as u16 as i16),
            | _ => unsigned as i32,
        }
    }
}

/// One decoded HID long item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LongItem<'a> {
    tag:  u8,
    data: &'a [u8],
}

impl<'a> LongItem<'a> {
    /// Returns the long-item tag byte.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self.tag
    }

    /// Returns the long-item payload.
    #[must_use]
    pub const fn data(self) -> &'a [u8] {
        self.data
    }
}

/// Any decoded report-descriptor item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Item<'a> {
    /// A standard short item, including unknown type/tag combinations.
    Short(ShortItem<'a>),
    /// The `0xFE` long-item form.
    Long(LongItem<'a>),
}

/// A decoded item together with its byte offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpannedItem<'a> {
    /// Offset of the item's prefix from the descriptor start.
    pub offset: usize,
    /// Decoded item.
    pub item:   Item<'a>,
}

/// An allocation-free descriptor parser error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    /// The descriptor ended before the complete item was available.
    UnexpectedEnd {
        /// Item offset.
        offset:    usize,
        /// Total bytes required from the item offset.
        needed:    usize,
        /// Bytes actually available from the item offset.
        available: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            | Self::UnexpectedEnd {
                offset,
                needed,
                available,
            } => {
                write!(
                    f,
                    "Truncated HID item at byte {offset}: need {needed} bytes, have {available}"
                )
            },
        }
    }
}

impl core::error::Error for ParseError {}

/// Streaming, allocation-free iterator over report-descriptor items.
#[derive(Clone, Debug)]
pub struct Items<'a> {
    bytes:    &'a [u8],
    offset:   usize,
    finished: bool,
}

impl<'a> Items<'a> {
    /// Creates a parser over `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            finished: false,
        }
    }

    /// Returns the next unread byte offset.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

impl<'a> Iterator for Items<'a> {
    type Item = Result<SpannedItem<'a>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished || self.offset >= self.bytes.len() {
            return None;
        }

        let start = self.offset;
        let prefix = self.bytes[start];
        let available = self.bytes.len() - start;

        if prefix == LONG_ITEM_PREFIX {
            if available < 3 {
                self.finished = true;
                return Some(Err(ParseError::UnexpectedEnd {
                    offset: start,
                    needed: 3,
                    available,
                }));
            }
            let length = usize::from(self.bytes[start + 1]);
            let needed = 3 + length;
            if available < needed {
                self.finished = true;
                return Some(Err(ParseError::UnexpectedEnd {
                    offset: start,
                    needed,
                    available,
                }));
            }
            let tag = self.bytes[start + 2];
            let data = &self.bytes[start + 3 .. start + needed];
            self.offset += needed;
            return Some(Ok(SpannedItem {
                offset: start,
                item:   Item::Long(LongItem {
                    tag,
                    data,
                }),
            }));
        }

        let size = DataSize::from_prefix_code(prefix).bytes();
        let needed = 1 + size;
        if available < needed {
            self.finished = true;
            return Some(Err(ParseError::UnexpectedEnd {
                offset: start,
                needed,
                available,
            }));
        }

        let item_type = ItemType::from_bits(prefix >> 2);
        let tag = (prefix >> 4) & 0x0F;
        let data = &self.bytes[start + 1 .. start + needed];
        self.offset += needed;
        Some(Ok(SpannedItem {
            offset: start,
            item:   Item::Short(ShortItem {
                prefix,
                item_type,
                tag,
                data,
            }),
        }))
    }
}

/// Width selection for numeric short items.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeWidth {
    /// Select the smallest legal width that preserves the value.
    Auto,
    /// Use exactly this HID short-item size.
    Exact(DataSize),
}

/// A short-item encoding error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    /// The tag did not fit in four bits.
    TagOutOfRange(u8),
    /// Payload length was not 0, 1, 2, or 4 bytes.
    InvalidPayloadLength(usize),
    /// A numeric value does not fit the requested width.
    ValueDoesNotFit(DataSize),
    /// The encoded prefix would be `0xFE`, which is reserved for long items.
    LongItemPrefixCollision,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            | Self::TagOutOfRange(tag) => {
                write!(f, "Short-item tag {tag:#x} exceeds four bits")
            },
            | Self::InvalidPayloadLength(length) => {
                write!(f, "Short-item payload length {length} is not 0, 1, 2, or 4")
            },
            | Self::ValueDoesNotFit(size) => {
                write!(f, "Value does not fit requested {}-byte width", size.bytes())
            },
            | Self::LongItemPrefixCollision => {
                f.write_str("Short-item prefix 0xfe is reserved for the HID long-item format")
            },
        }
    }
}

impl core::error::Error for EncodeError {}

/// An encoded HID short item backed by an inline five-byte buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodedShort {
    bytes:  [u8; MAX_SHORT_ITEM_LEN],
    length: u8,
}

impl EncodedShort {
    /// Returns only the initialized prefix and payload bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[.. usize::from(self.length)]
    }

    /// Returns the encoded length including the prefix.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.length as usize
    }

    /// Returns whether this encoding has no bytes. Short items are never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

/// Encodes an arbitrary short item while validating its structural width.
///
/// # Errors
///
/// Returns [`EncodeError`] when `tag` exceeds four bits, `data` is not a legal
/// HID short-item length (0, 1, 2, or 4 bytes), or the prefix would be `0xFE`.
pub fn encode_short_raw(item_type: ItemType, tag: u8, data: &[u8]) -> Result<EncodedShort, EncodeError> {
    if tag > 0x0F {
        return Err(EncodeError::TagOutOfRange(tag));
    }
    let Some(size) = DataSize::from_byte_count(data.len()) else {
        return Err(EncodeError::InvalidPayloadLength(data.len()));
    };
    let prefix = (tag << 4) | ((item_type as u8) << 2) | size.prefix_code();
    if prefix == LONG_ITEM_PREFIX {
        return Err(EncodeError::LongItemPrefixCollision);
    }
    let mut bytes = [0_u8; MAX_SHORT_ITEM_LEN];
    bytes[0] = prefix;
    bytes[1 ..= data.len()].copy_from_slice(data);
    Ok(EncodedShort {
        bytes,
        length: (1 + data.len()) as u8,
    })
}

/// Encodes an unsigned numeric short item.
///
/// # Errors
///
/// Returns [`EncodeError`] when the tag or requested width is invalid, or when
/// `value` does not fit `width`.
pub fn encode_unsigned(
    item_type: ItemType,
    tag: u8,
    value: u32,
    width: EncodeWidth,
) -> Result<EncodedShort, EncodeError> {
    let size = match width {
        | EncodeWidth::Auto => {
            if value <= u32::from(u8::MAX) {
                DataSize::One
            } else if value <= u32::from(u16::MAX) {
                DataSize::Two
            } else {
                DataSize::Four
            }
        },
        | EncodeWidth::Exact(size) => size,
    };
    let fits = match size {
        | DataSize::Zero => value == 0,
        | DataSize::One => value <= u32::from(u8::MAX),
        | DataSize::Two => value <= u32::from(u16::MAX),
        | DataSize::Four => true,
    };
    if !fits {
        return Err(EncodeError::ValueDoesNotFit(size));
    }
    let bytes = value.to_le_bytes();
    encode_short_raw(item_type, tag, &bytes[.. size.bytes()])
}

/// Encodes a signed numeric short item using two's-complement representation.
///
/// # Errors
///
/// Returns [`EncodeError`] when the tag or requested width is invalid, or when
/// `value` does not fit `width`.
pub fn encode_signed(
    item_type: ItemType,
    tag: u8,
    value: i32,
    width: EncodeWidth,
) -> Result<EncodedShort, EncodeError> {
    let size = match width {
        | EncodeWidth::Auto => {
            if i32::from(i8::MIN) <= value && value <= i32::from(i8::MAX) {
                DataSize::One
            } else if i32::from(i16::MIN) <= value && value <= i32::from(i16::MAX) {
                DataSize::Two
            } else {
                DataSize::Four
            }
        },
        | EncodeWidth::Exact(size) => size,
    };
    let fits = match size {
        | DataSize::Zero => value == 0,
        | DataSize::One => i32::from(i8::MIN) <= value && value <= i32::from(i8::MAX),
        | DataSize::Two => i32::from(i16::MIN) <= value && value <= i32::from(i16::MAX),
        | DataSize::Four => true,
    };
    if !fits {
        return Err(EncodeError::ValueDoesNotFit(size));
    }
    let bytes = value.to_le_bytes();
    encode_short_raw(item_type, tag, &bytes[.. size.bytes()])
}

/// Standard bit flags carried by Input, Output, and Feature Main items.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct MainFlags(u32);

impl MainFlags {
    /// Buffered Bytes rather than Bit Field (bit 8).
    pub const BUFFERED_BYTES: u32 = 1 << 8;
    /// Constant rather than Data.
    pub const CONSTANT: u32 = 1 << 0;
    /// Non Linear rather than Linear.
    pub const NON_LINEAR: u32 = 1 << 4;
    /// No Preferred State rather than Preferred State.
    pub const NO_PREFERRED: u32 = 1 << 5;
    /// Null State rather than No Null Position.
    pub const NULL_STATE: u32 = 1 << 6;
    /// Relative rather than Absolute.
    pub const RELATIVE: u32 = 1 << 2;
    /// Variable rather than Array.
    pub const VARIABLE: u32 = 1 << 1;
    /// Volatile rather than Non Volatile (Output/Feature bit 7).
    pub const VOLATILE: u32 = 1 << 7;
    /// Wrap rather than No Wrap.
    pub const WRAP: u32 = 1 << 3;

    /// Creates flags from the complete raw bit field, retaining reserved bits.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns all raw bits, including unknown future bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns whether every requested bit is set.
    #[must_use]
    pub const fn contains(self, bits: u32) -> bool {
        self.0 & bits == bits
    }
}

impl fmt::Debug for MainFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MainFlags({:#x})", self.0)
    }
}

/// A collection type byte. Values outside the standard range are retained.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CollectionType(u8);

impl CollectionType {
    /// Application collection.
    pub const APPLICATION: Self = Self(0x01);
    /// Logical collection.
    pub const LOGICAL: Self = Self(0x02);
    /// Named Array collection.
    pub const NAMED_ARRAY: Self = Self(0x04);
    /// Physical collection.
    pub const PHYSICAL: Self = Self(0x00);
    /// Report collection.
    pub const REPORT: Self = Self(0x03);
    /// Usage Modifier collection.
    pub const USAGE_MODIFIER: Self = Self(0x06);
    /// Usage Switch collection.
    pub const USAGE_SWITCH: Self = Self(0x05);

    /// Creates a collection type from any raw byte.
    #[must_use]
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// Returns the original byte.
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Returns whether this is in the vendor-defined `0x80..=0xff` range.
    #[must_use]
    pub const fn is_vendor_defined(self) -> bool {
        self.0 >= 0x80
    }
}

/// A complete 32-bit HID usage (`page << 16 | id`).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Usage(u32);

impl Usage {
    /// Constructs a usage from its numeric page and identifier.
    #[must_use]
    pub const fn new(page: u16, id: u16) -> Self {
        Self(((page as u32) << 16) | id as u32)
    }

    /// Constructs a usage from its raw 32-bit representation.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the usage page.
    #[must_use]
    pub const fn page(self) -> u16 {
        (self.0 >> 16) as u16
    }

    /// Returns the usage identifier within the page.
    #[must_use]
    pub const fn id(self) -> u16 {
        self.0 as u16
    }

    /// Returns the complete 32-bit value.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// HID unit system nibble.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum UnitSystem {
    /// No unit system.
    None            = 0,
    /// SI linear.
    SiLinear        = 1,
    /// SI rotation.
    SiRotation      = 2,
    /// English linear.
    EnglishLinear   = 3,
    /// English rotation.
    EnglishRotation = 4,
}

/// Error returned when a unit exponent does not fit a signed four-bit nibble.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitExponentError(pub i8);

impl fmt::Display for UnitExponentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HID unit exponent {} is outside the signed four-bit range -8..=7",
            self.0
        )
    }
}

impl core::error::Error for UnitExponentError {}

/// Packed HID Unit item value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Unit(u32);

impl Unit {
    /// No unit.
    pub const NONE: Self = Self(0);

    /// Constructs a packed Unit from the raw value, preserving future nibbles.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Packs the seven standard HID unit nibbles.
    ///
    /// # Errors
    ///
    /// Returns [`UnitExponentError`] when any exponent is outside `-8..=7`.
    pub fn new(
        system: UnitSystem,
        length: i8,
        mass: i8,
        time: i8,
        temperature: i8,
        current: i8,
        luminous_intensity: i8,
    ) -> Result<Self, UnitExponentError> {
        let mut raw = u32::from(system as u8);
        let exponents = [length, mass, time, temperature, current, luminous_intensity];
        let mut index = 0_usize;
        while index < exponents.len() {
            let exponent = exponents[index];
            if !(-8 ..= 7).contains(&exponent) {
                return Err(UnitExponentError(exponent));
            }
            raw |= u32::from((exponent as u8) & 0x0F) << ((index + 1) * 4);
            index += 1;
        }
        Ok(Self(raw))
    }

    /// Returns the packed Unit item value.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Decodes one signed four-bit exponent nibble (`1..=6`).
    #[must_use]
    pub const fn exponent(self, nibble: u8) -> Option<i8> {
        if nibble == 0 || nibble > 6 {
            return None;
        }
        let value = ((self.0 >> (nibble as u32 * 4)) & 0x0F) as i8;
        Some(if value & 0x08 != 0 {
            value - 16
        } else {
            value
        })
    }
}

/// Decodes the signed four-bit Unit Exponent item value.
#[must_use]
pub const fn decode_unit_exponent(raw: u32) -> i8 {
    let nibble = (raw & 0x0F) as i8;
    if nibble & 0x08 != 0 {
        nibble - 16
    } else {
        nibble
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_and_long_items() {
        let bytes = [0x05, 0x01, 0xFE, 0x02, 0xAA, 0x10, 0x20, 0xC0];
        let parsed: heapless_test::Vec<_, 3> = Items::new(&bytes).collect();
        assert_eq!(parsed.len(), 3);
        assert!(matches!(
            parsed[0],
            Ok(SpannedItem {
                item: Item::Short(_),
                ..
            })
        ));
        assert!(matches!(
            parsed[1],
            Ok(SpannedItem {
                item: Item::Long(_),
                ..
            })
        ));
    }

    #[test]
    fn numeric_encoders_round_trip_every_short_width() {
        let cases = [
            (0_u32, DataSize::Zero),
            (0x7F, DataSize::One),
            (0x1234, DataSize::Two),
            (0x89AB_CDEF, DataSize::Four),
        ];
        for (value, size) in cases {
            let encoded = encode_unsigned(ItemType::Reserved, 0x0E, value, EncodeWidth::Exact(size)).unwrap();
            let parsed = Items::new(encoded.as_slice()).next().unwrap().unwrap();
            let Item::Short(item) = parsed.item else {
                panic!("Short encoding parsed as a long item");
            };
            assert_eq!(item.size(), size);
            assert_eq!(item.unsigned_value(), value);
            assert!(matches!(item.tag(), ItemTag::Unknown {
                item_type: ItemType::Reserved,
                tag:       0x0E,
            }));
        }
    }

    #[test]
    fn rejects_the_long_item_prefix_collision() {
        assert_eq!(
            encode_short_raw(ItemType::Reserved, 0x0F, &[1, 2]),
            Err(EncodeError::LongItemPrefixCollision)
        );
    }

    #[test]
    fn signed_encoder_selects_twos_complement_width() {
        let encoded = encode_signed(
            ItemType::Global,
            GlobalTag::LogicalMinimum as u8,
            -129,
            EncodeWidth::Auto,
        )
        .unwrap();
        assert_eq!(encoded.as_slice(), &[0x16, 0x7F, 0xFF]);
        let parsed = Items::new(encoded.as_slice()).next().unwrap().unwrap();
        let Item::Short(item) = parsed.item else {
            panic!("Expected short item");
        };
        assert_eq!(item.signed_value(), -129);
    }

    #[test]
    fn reports_truncated_long_item_once() {
        let mut items = Items::new(&[0xFE, 3, 0x44, 1]);
        assert!(matches!(
            items.next(),
            Some(Err(ParseError::UnexpectedEnd {
                offset:    0,
                needed:    6,
                available: 4,
            }))
        ));
        assert!(items.next().is_none());
    }

    #[test]
    fn unit_nibbles_preserve_signed_exponents() {
        let unit = Unit::new(UnitSystem::SiLinear, 1, 0, -2, 0, 0, 0).unwrap();
        assert_eq!(unit.exponent(1), Some(1));
        assert_eq!(unit.exponent(3), Some(-2));
        assert_eq!(unit.exponent(0), None);
        assert_eq!(decode_unit_exponent(0x0E), -2);
    }

    // Tiny test-only fixed vector avoids an alloc dependency in this no_std crate.
    mod heapless_test {
        pub struct Vec<T, const N: usize> {
            values: [Option<T>; N],
            len:    usize,
        }

        impl<T, const N: usize> Vec<T, N> {
            pub fn len(&self) -> usize {
                self.len
            }
        }

        impl<T, const N: usize> core::ops::Index<usize> for Vec<T, N> {
            type Output = T;

            fn index(&self, index: usize) -> &Self::Output {
                self.values[index].as_ref().expect("Index initialized")
            }
        }

        impl<T, const N: usize> FromIterator<T> for Vec<T, N> {
            fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
                let mut values = core::array::from_fn(|_| None);
                let mut len = 0;
                for item in iter {
                    assert!(len < N);
                    values[len] = Some(item);
                    len += 1;
                }
                Self {
                    values,
                    len,
                }
            }
        }
    }
}
