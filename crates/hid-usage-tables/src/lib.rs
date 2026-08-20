#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! USB HID Usage Tables as numeric, metadata-rich records.
//!
//! Unlike enum-only APIs, this catalog preserves page and usage numbers,
//! multi-valued Usage Types, generated ranges, reserved and vendor-defined
//! page ranges, aliases, and status metadata. Unknown numeric values remain
//! representable through [`hidder_core::Usage`].
//!
//! ```
//! use hid_usage_tables::{
//!     UsageLookup,
//!     UsageType,
//!     lookup_usage,
//! };
//! use hidder_core::Usage;
//!
//! let UsageLookup::Defined(entry) = lookup_usage(Usage::new(0x0001, 0x0030)) else {
//!     panic!("X is individually defined");
//! };
//! assert_eq!(entry.name, "X");
//! assert!(entry.types.contains(UsageType::DynamicValue));
//! ```

use core::{
    cmp::Ordering,
    fmt,
};

use hidder_core::Usage;

#[rustfmt::skip]
mod generated;

pub use generated::{
    ALIASES,
    HID_SPEC_URL,
    HID_SPEC_VERSION,
    HUT_JSON_ATTACHMENT,
    HUT_JSON_SHA256,
    HUT_LAST_GENERATED,
    HUT_PUBLICATION_DATE,
    HUT_SOURCE_URL,
    HUT_VERSION,
    NAMED_USAGE_COUNT,
    PAGE_COUNT,
    PAGE_RANGES,
    PAGES,
    USAGE_RANGES,
    USAGES,
};

/// Inclusive 16-bit identifier range.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IdRange {
    /// First value in the range.
    pub start: u16,
    /// Last value in the range.
    pub end:   u16,
}

impl IdRange {
    /// Creates an inclusive range.
    #[must_use]
    pub const fn new(start: u16, end: u16) -> Self {
        Self {
            start,
            end,
        }
    }

    /// Returns whether `value` lies in the range.
    #[must_use]
    pub const fn contains(self, value: u16) -> bool {
        self.start <= value && value <= self.end
    }
}

/// Provenance/classification of a usage page record.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PageClass {
    /// Page zero, explicitly undefined by the specification.
    Undefined,
    /// A page containing individually defined usage records.
    Defined,
    /// A page defined by a numeric generator rather than a finite list.
    Generated,
    /// A page named by the HUT page summary but specified elsewhere.
    External,
}

/// Classification of an unallocated page range.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PageRangeKind {
    /// Reserved by the cited HUT revision.
    Reserved,
    /// Available for vendor-defined usage pages.
    VendorDefined,
}

/// One page range from the HUT page summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageRangeEntry {
    /// Inclusive numeric page range.
    pub range: IdRange,
    /// Reserved or vendor-defined classification.
    pub kind:  PageRangeKind,
    /// Human-readable source label.
    pub label: &'static str,
}

/// Classification of an unassigned usage-ID range within a page.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UsageRangeKind {
    /// Explicitly or implicitly reserved/unassigned in the source revision.
    Reserved,
}

/// A usage-ID range associated with one page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsageRangeEntry {
    /// Numeric usage page.
    pub page:  u16,
    /// Inclusive usage-ID range.
    pub range: IdRange,
    /// Range classification.
    pub kind:  UsageRangeKind,
}

/// Status attached to a usage record.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UsageStatus {
    /// Current in the generated HUT revision.
    Current,
    /// Present for compatibility but marked deprecated by the specification.
    Deprecated,
}

/// Provenance of an alternate name.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AliasKind {
    /// Alternate spelling printed in the source specification.
    Specification,
    /// Name retained from an earlier revision or compatibility source.
    Legacy,
    /// Search-friendly shorthand derived without changing the canonical name.
    Search,
}

/// One alternate usage name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AliasEntry {
    /// Usage page.
    pub page: u16,
    /// Usage identifier.
    pub id:   u16,
    /// Alternate name.
    pub name: &'static str,
    /// Alias provenance.
    pub kind: AliasKind,
}

/// One Usage Type from HUT section 3.4.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum UsageType {
    /// Linear Control (`LC`).
    LinearControl,
    /// On/Off Control (`OOC`).
    OnOffControl,
    /// Momentary Control (`MC`).
    MomentaryControl,
    /// One Shot Control (`OSC`).
    OneShotControl,
    /// Re-Trigger Control (`RTC`).
    RetriggerControl,
    /// Selector (`Sel`).
    Selector,
    /// Static Value (`SV`).
    StaticValue,
    /// Static Flag (`SF`).
    StaticFlag,
    /// Dynamic Value (`DV`).
    DynamicValue,
    /// Dynamic Flag (`DF`).
    DynamicFlag,
    /// Named Array (`NAry`).
    NamedArray,
    /// Application Collection (`CA`).
    ApplicationCollection,
    /// Logical Collection (`CL`).
    LogicalCollection,
    /// Physical Collection (`CP`).
    PhysicalCollection,
    /// Usage Switch (`US`).
    UsageSwitch,
    /// Usage Modifier (`UM`).
    UsageModifier,
    /// Buffered Bytes data.
    BufferedBytes,
}

impl UsageType {
    const ALL: [Self; 17] = [
        Self::LinearControl,
        Self::OnOffControl,
        Self::MomentaryControl,
        Self::OneShotControl,
        Self::RetriggerControl,
        Self::Selector,
        Self::StaticValue,
        Self::StaticFlag,
        Self::DynamicValue,
        Self::DynamicFlag,
        Self::NamedArray,
        Self::ApplicationCollection,
        Self::LogicalCollection,
        Self::PhysicalCollection,
        Self::UsageSwitch,
        Self::UsageModifier,
        Self::BufferedBytes,
    ];

    /// Returns the source abbreviation.
    #[must_use]
    pub const fn abbreviation(self) -> &'static str {
        match self {
            | Self::LinearControl => "LC",
            | Self::OnOffControl => "OOC",
            | Self::MomentaryControl => "MC",
            | Self::OneShotControl => "OSC",
            | Self::RetriggerControl => "RTC",
            | Self::Selector => "Sel",
            | Self::StaticValue => "SV",
            | Self::StaticFlag => "SF",
            | Self::DynamicValue => "DV",
            | Self::DynamicFlag => "DF",
            | Self::NamedArray => "NAry",
            | Self::ApplicationCollection => "CA",
            | Self::LogicalCollection => "CL",
            | Self::PhysicalCollection => "CP",
            | Self::UsageSwitch => "US",
            | Self::UsageModifier => "UM",
            | Self::BufferedBytes => "BufferedBytes",
        }
    }

    const fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// Bitset retaining every Usage Type attached to a source entry.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct UsageTypeSet(u32);

impl UsageTypeSet {
    /// Empty type set.
    pub const EMPTY: Self = Self(0);

    /// Creates a type set from generated raw bits.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the complete bitset.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns whether this set contains `usage_type`.
    #[must_use]
    pub const fn contains(self, usage_type: UsageType) -> bool {
        self.0 & usage_type.bit() != 0
    }

    /// Iterates over all present Usage Types in stable source order.
    #[must_use]
    pub const fn iter(self) -> UsageTypeIter {
        UsageTypeIter {
            set: self, index: 0
        }
    }
}

impl fmt::Debug for UsageTypeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// Iterator returned by [`UsageTypeSet::iter`].
#[derive(Clone, Debug)]
pub struct UsageTypeIter {
    set:   UsageTypeSet,
    index: usize,
}

impl Iterator for UsageTypeIter {
    type Item = UsageType;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < UsageType::ALL.len() {
            let value = UsageType::ALL[self.index];
            self.index += 1;
            if self.set.contains(value) {
                return Some(value);
            }
        }
        None
    }
}

/// Metadata for an algorithmically generated usage-ID range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsageGenerator {
    /// Textual prefix, such as `Button` or `Instance`.
    pub name_prefix: &'static str,
    /// Inclusive generated identifier range.
    pub range:       IdRange,
    /// Usage Types applying to generated values.
    pub types:       UsageTypeSet,
}

/// One named usage from the machine-readable source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsageEntry {
    /// Numeric usage page.
    pub page:        u16,
    /// Numeric usage identifier.
    pub id:          u16,
    /// Canonical source name.
    pub name:        &'static str,
    /// Stable generated ASCII slug suitable for code and lookup.
    pub slug:        &'static str,
    /// All source Usage Types; entries are not forced into one enum variant.
    pub types:       UsageTypeSet,
    /// Current/deprecated state.
    pub status:      UsageStatus,
    /// Index of this entry's first alias in [`ALIASES`].
    pub alias_start: u32,
    /// Number of aliases belonging to this entry.
    pub alias_len:   u16,
}

impl UsageEntry {
    /// Returns the complete 32-bit HID usage.
    #[must_use]
    pub const fn usage(self) -> Usage {
        Usage::new(self.page, self.id)
    }

    /// Returns all aliases for this record.
    #[must_use]
    pub fn aliases(self) -> &'static [AliasEntry] {
        let start = self.alias_start as usize;
        &ALIASES[start .. start + self.alias_len as usize]
    }
}

/// One usage page and its indexed ranges in the generated arrays.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsagePageEntry {
    /// Numeric page identifier.
    pub id:          u16,
    /// Canonical page name.
    pub name:        &'static str,
    /// Stable ASCII slug.
    pub slug:        &'static str,
    /// Page definition class.
    pub class:       PageClass,
    /// First named usage in [`USAGES`].
    pub usage_start: u32,
    /// Number of named usages.
    pub usage_len:   u16,
    /// Optional generated range.
    pub generator:   Option<UsageGenerator>,
    /// First reserved usage range in [`USAGE_RANGES`].
    pub range_start: u32,
    /// Number of usage ranges for this page.
    pub range_len:   u16,
}

impl UsagePageEntry {
    /// Returns the page's named usage records.
    #[must_use]
    pub fn usages(self) -> &'static [UsageEntry] {
        let start = self.usage_start as usize;
        &USAGES[start .. start + self.usage_len as usize]
    }

    /// Returns known reserved/unassigned usage ranges for this page.
    #[must_use]
    pub fn ranges(self) -> &'static [UsageRangeEntry] {
        let start = self.range_start as usize;
        &USAGE_RANGES[start .. start + self.range_len as usize]
    }
}

/// Result of resolving a numeric page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageLookup {
    /// Named page metadata.
    Page(&'static UsagePageEntry),
    /// Reserved page range.
    Reserved(&'static PageRangeEntry),
    /// Vendor-defined page range.
    VendorDefined(&'static PageRangeEntry),
    /// Not classified by this source revision.
    Unknown(u16),
}

/// A concrete member of a generated page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedUsage {
    /// Complete numeric usage.
    pub usage:     Usage,
    /// Generator metadata.
    pub generator: &'static UsageGenerator,
}

impl fmt::Display for GeneratedUsage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.generator.name_prefix, self.usage.id())
    }
}

/// Result of resolving a numeric usage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageLookup {
    /// Individually defined usage.
    Defined(&'static UsageEntry),
    /// Member of an algorithmically generated range.
    Generated(GeneratedUsage),
    /// Reserved/unassigned range in a known page.
    Reserved(&'static UsageRangeEntry),
    /// Page is known but its external definition is not in the HUT JSON.
    External(Usage),
    /// Page zero, which the specification classifies as undefined.
    Undefined(Usage),
    /// The usage page itself lies in a reserved page range.
    ReservedPage {
        /// Complete numeric usage.
        usage: Usage,
        /// Page-range metadata from the HUT page summary.
        range: &'static PageRangeEntry,
    },
    /// Vendor-defined page.
    VendorDefined(Usage),
    /// Unknown to this source revision.
    Unknown(Usage),
}

/// Resolves a numeric usage page without losing range classification.
#[must_use]
pub fn lookup_page(id: u16) -> PageLookup {
    let mut low = 0_usize;
    let mut high = PAGES.len();
    while low < high {
        let middle = low + (high - low) / 2;
        let page = &PAGES[middle];
        match page.id.cmp(&id) {
            | Ordering::Less => low = middle + 1,
            | Ordering::Greater => high = middle,
            | Ordering::Equal => return PageLookup::Page(page),
        }
    }
    for range in PAGE_RANGES {
        if range.range.contains(id) {
            return match range.kind {
                | PageRangeKind::Reserved => PageLookup::Reserved(range),
                | PageRangeKind::VendorDefined => PageLookup::VendorDefined(range),
            };
        }
    }
    PageLookup::Unknown(id)
}

/// Finds a page by canonical name or generated slug.
#[must_use]
pub fn find_page(name: &str) -> Option<&'static UsagePageEntry> {
    PAGES
        .iter()
        .find(|page| eq_normalized(name, page.name) || eq_normalized(name, page.slug))
}

/// Resolves a complete numeric usage, including generated and reserved ranges.
#[must_use]
pub fn lookup_usage(usage: Usage) -> UsageLookup {
    let page = usage.page();
    let id = usage.id();
    match lookup_page(page) {
        | PageLookup::Page(page_entry) => {
            let usages = page_entry.usages();
            let mut low = 0_usize;
            let mut high = usages.len();
            while low < high {
                let middle = low + (high - low) / 2;
                let entry = &usages[middle];
                match entry.id.cmp(&id) {
                    | Ordering::Less => low = middle + 1,
                    | Ordering::Greater => high = middle,
                    | Ordering::Equal => return UsageLookup::Defined(entry),
                }
            }
            if let Some(generator) = page_entry.generator.as_ref() {
                if generator.range.contains(id) {
                    return UsageLookup::Generated(GeneratedUsage {
                        usage,
                        generator,
                    });
                }
            }
            for range in page_entry.ranges() {
                if range.range.contains(id) {
                    return UsageLookup::Reserved(range);
                }
            }
            if page_entry.class == PageClass::External {
                UsageLookup::External(usage)
            } else if page_entry.class == PageClass::Undefined {
                UsageLookup::Undefined(usage)
            } else {
                UsageLookup::Unknown(usage)
            }
        },
        | PageLookup::VendorDefined(_) => UsageLookup::VendorDefined(usage),
        | PageLookup::Reserved(range) => {
            UsageLookup::ReservedPage {
                usage,
                range,
            }
        },
        | PageLookup::Unknown(_) => UsageLookup::Unknown(usage),
    }
}

/// Finds a usage by name, slug, or recorded alias within `page`.
#[must_use]
pub fn find_usage(page: u16, name: &str) -> Option<Usage> {
    let PageLookup::Page(page_entry) = lookup_page(page) else {
        return None;
    };
    for entry in page_entry.usages() {
        if eq_normalized(name, entry.name) || eq_normalized(name, entry.slug) {
            return Some(entry.usage());
        }
        if entry.aliases().iter().any(|alias| eq_normalized(name, alias.name)) {
            return Some(entry.usage());
        }
    }
    if let Some(generator) = page_entry.generator.as_ref() {
        if let Some(id) = parse_generated_name(name, generator.name_prefix) {
            if generator.range.contains(id) {
                return Some(Usage::new(page, id));
            }
        }
    }
    None
}

/// ASCII-only comparison that ignores separators and case.
#[must_use]
pub fn eq_normalized(left: &str, right: &str) -> bool {
    let mut left = left
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| byte.to_ascii_lowercase());
    let mut right = right
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| byte.to_ascii_lowercase());
    loop {
        match (left.next(), right.next()) {
            | (None, None) => return true,
            | (Some(a), Some(b)) if a == b => {},
            | _ => return false,
        }
    }
}

fn parse_generated_name(name: &str, prefix: &str) -> Option<u16> {
    let mut significant = name.bytes().filter(u8::is_ascii_alphanumeric);
    for expected in prefix.bytes().filter(u8::is_ascii_alphanumeric) {
        let actual = significant.next()?;
        if !actual.eq_ignore_ascii_case(&expected) {
            return None;
        }
    }
    let mut value = 0_u32;
    let mut any = false;
    for byte in significant {
        if !byte.is_ascii_digit() {
            return None;
        }
        any = true;
        value = value.checked_mul(10)?.checked_add(u32::from(byte - b'0'))?;
    }
    any.then_some(value).and_then(|value| u16::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_catalog_indices_are_consistent() {
        assert_eq!(NAMED_USAGE_COUNT, USAGES.len());
        assert_eq!(PAGE_COUNT, PAGES.len());
        assert!(PAGES.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert!(
            USAGES
                .windows(2)
                .all(|pair| { (pair[0].page, pair[0].id) < (pair[1].page, pair[1].id) })
        );

        for page in PAGES {
            assert!(page.usage_start as usize + page.usage_len as usize <= USAGES.len());
            assert!(page.range_start as usize + page.range_len as usize <= USAGE_RANGES.len());
            assert!(page.usages().iter().all(|usage| usage.page == page.id));
            assert!(page.ranges().iter().all(|range| range.page == page.id));
        }
        for usage in USAGES {
            assert!(usage.alias_start as usize + usage.alias_len as usize <= ALIASES.len());
            assert!(
                usage
                    .aliases()
                    .iter()
                    .all(|alias| alias.page == usage.page && alias.id == usage.id)
            );
        }
    }

    #[test]
    fn lookup_retains_defined_generated_and_range_metadata() {
        let mouse = Usage::new(0x0001, 0x0002);
        let UsageLookup::Defined(entry) = lookup_usage(mouse) else {
            panic!("Mouse should be individually defined");
        };
        assert_eq!(entry.name, "Mouse");
        assert!(entry.types.contains(UsageType::ApplicationCollection));

        let button = Usage::new(0x0009, 42);
        let UsageLookup::Generated(generated) = lookup_usage(button) else {
            panic!("Button 42 should be generated");
        };
        assert_eq!(generated.generator.name_prefix, "Button");
        assert!(generated.generator.types.contains(UsageType::Selector));

        assert!(matches!(lookup_usage(Usage::new(0x0001, 0)), UsageLookup::Reserved(_)));
        assert!(matches!(
            lookup_usage(Usage::new(0x0013, 1)),
            UsageLookup::ReservedPage { .. }
        ));
        assert!(matches!(
            lookup_usage(Usage::new(0x0010, 0x0041)),
            UsageLookup::External(_)
        ));
        assert!(matches!(lookup_usage(Usage::new(0x0000, 1)), UsageLookup::Undefined(_)));
        assert!(matches!(
            lookup_usage(Usage::new(0xFF00, 1)),
            UsageLookup::VendorDefined(_)
        ));
    }

    #[test]
    fn names_aliases_and_generated_names_are_searchable() {
        assert_eq!(find_page("generic-desktop").map(|page| page.id), Some(0x0001));
        assert_eq!(find_usage(0x0001, "Game Pad"), Some(Usage::new(0x0001, 0x0005)));
        assert_eq!(find_usage(0x0007, "Enter"), Some(Usage::new(0x0007, 0x0028)));
        assert_eq!(find_usage(0x0009, "Button 65535"), Some(Usage::new(0x0009, 0xFFFF)));
        assert_eq!(find_usage(0x0009, "Button 65536"), None);
    }

    #[test]
    fn deprecation_and_source_identity_are_retained() {
        let UsageLookup::Defined(entry) = lookup_usage(Usage::new(0x000F, 0x0059)) else {
            panic!("Expected PID block handle metadata");
        };
        assert_eq!(entry.status, UsageStatus::Deprecated);
        assert_eq!(HUT_VERSION, "1.7.0");
        assert_eq!(HID_SPEC_VERSION, "1.11");
        assert_eq!(HUT_JSON_SHA256.len(), 64);
    }

    #[test]
    fn usage_type_bits_have_stable_discriminants() {
        for (index, usage_type) in UsageType::ALL.iter().copied().enumerate() {
            assert_eq!(usage_type as usize, index);
            assert_eq!(UsageTypeSet::from_bits(1_u32 << index).iter().next(), Some(usage_type));
        }
    }
}
