#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Semantic USB HID report descriptor parser.
//!
//! [`hidder_core::Items`] provides the allocation-free item stream. This
//! crate builds collections, report layouts, usage sets, strings/designators,
//! and complete Global state snapshots using `alloc`, while retaining long and
//! unknown items for forward compatibility.
//!
//! Parse records are stored in any [`RecordBuf`]. [`parse`] uses [`VecStore`];
//! [`parse_in`] accepts [`VecDequeStore`], [`LinkedListStore`], or a custom
//! store.
//!
//! ```
//! use hidder_parser::{
//!     VecDequeStore,
//!     parse,
//!     parse_in,
//! };
//!
//! let bytes = [0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0xC0];
//! let descriptor = parse(&bytes).unwrap();
//! assert_eq!(descriptor.collections.len(), 1);
//!
//! let queued = parse_in::<VecDequeStore>(&bytes).unwrap();
//! assert_eq!(queued.collections.len(), 1);
//! ```

extern crate alloc;

mod store;

use alloc::{
    string::{
        String,
        ToString,
    },
    vec,
    vec::Vec,
};

use hid_usage_tables::{
    UsageLookup,
    lookup_usage,
};
use hidder_core::{
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
    ShortItem,
    Usage,
    decode_unit_exponent,
};
pub use store::{
    DescriptorStore,
    LinkedListStore,
    ParseFailure,
    RecordBuf,
    VecDequeStore,
    VecStore,
};

/// Input, Output, or Feature report namespace.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReportKind {
    /// Host reads this report from the device.
    Input,
    /// Host writes this report to the device.
    Output,
    /// Bidirectional control/configuration report.
    Feature,
}

/// Severity of a semantic parser diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticLevel {
    /// Descriptor is structurally or semantically invalid.
    Error,
    /// Descriptor can be represented but is suspicious.
    Warning,
}

/// One parser diagnostic associated with a byte offset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// Error or warning.
    pub level:   DiagnosticLevel,
    /// Descriptor byte offset.
    pub offset:  usize,
    /// Human-readable message.
    pub message: String,
}

/// Parser strictness controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseOptions {
    /// Validate report sizes/counts, range ordering, item widths, and stacks.
    pub strict: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            strict: true
        }
    }
}

/// A report's final bit length, including its Report ID byte when non-zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportLayout {
    /// Report namespace.
    pub kind:        ReportKind,
    /// Zero for an unnumbered report, otherwise the Report ID.
    pub id:          u8,
    /// Total length in bits. Numbered reports begin with the eight-bit ID.
    pub bit_len:     u64,
    /// Number of Main fields contributing to this report.
    pub field_count: u32,
}

impl ReportLayout {
    /// Returns the byte length rounded up to the next whole byte.
    #[must_use]
    pub const fn byte_len(self) -> u64 {
        self.bit_len / 8
            + if self.bit_len.is_multiple_of(8) {
                0
            } else {
                1
            }
    }
}

/// Local Usage assignments attached to one Main item.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UsageAssignment {
    /// Primary set followed by any Delimiter alternate sets.
    pub sets:    Vec<Vec<Usage>>,
    /// Optional range minimum.
    pub minimum: Option<Usage>,
    /// Optional range maximum.
    pub maximum: Option<Usage>,
}

impl UsageAssignment {
    /// Returns the first explicitly assigned usage or the range minimum.
    #[must_use]
    pub fn primary(&self) -> Option<Usage> {
        self.sets.first().and_then(|set| set.first()).copied().or(self.minimum)
    }
}

/// Local designator assignments.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DesignatorAssignment {
    /// Explicit indices.
    pub indices: Vec<u32>,
    /// Range minimum.
    pub minimum: Option<u32>,
    /// Range maximum.
    pub maximum: Option<u32>,
}

/// Local string assignments.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StringAssignment {
    /// Explicit string indices.
    pub indices: Vec<u32>,
    /// Range minimum.
    pub minimum: Option<u32>,
    /// Range maximum.
    pub maximum: Option<u32>,
}

/// One Input, Output, or Feature Main item and its resolved Global/Local state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportField {
    /// Descriptor byte offset of the Main item.
    pub offset:           usize,
    /// Report namespace.
    pub kind:             ReportKind,
    /// Report ID, or zero for an unnumbered report.
    pub report_id:        u8,
    /// First bit of this field within the transmitted report.
    pub bit_offset:       u64,
    /// Size of one element in bits.
    pub report_size:      u32,
    /// Number of elements.
    pub report_count:     u32,
    /// Complete Main item bit field, including unknown future bits.
    pub flags:            MainFlags,
    /// Logical minimum.
    pub logical_minimum:  i32,
    /// Logical maximum, represented unsigned when the minimum is non-negative.
    pub logical_maximum:  i64,
    /// Physical minimum.
    pub physical_minimum: i32,
    /// Physical maximum.
    pub physical_maximum: i64,
    /// Signed four-bit unit exponent.
    pub unit_exponent:    i8,
    /// Packed HID Unit value.
    pub unit:             u32,
    /// Local usage assignments.
    pub usages:           UsageAssignment,
    /// Local designator assignments.
    pub designators:      DesignatorAssignment,
    /// Local string assignments.
    pub strings:          StringAssignment,
    /// Innermost collection index, if any.
    pub collection:       Option<usize>,
}

impl ReportField {
    /// Returns this Main item's total number of payload bits.
    #[must_use]
    pub const fn bit_len(&self) -> u64 {
        self.report_size as u64 * self.report_count as u64
    }

    /// Resolves the primary usage against the generated HUT metadata catalog.
    #[must_use]
    pub fn primary_usage_metadata(&self) -> Option<UsageLookup> {
        self.usages.primary().map(lookup_usage)
    }
}

/// One Collection Main item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Collection {
    /// Descriptor byte offset where the collection starts.
    pub start_offset:    usize,
    /// Offset of End Collection, if closed.
    pub end_offset:      Option<usize>,
    /// Standard, reserved, or vendor-defined collection byte.
    pub collection_type: CollectionType,
    /// Primary usage attached to the Collection Main item.
    pub usage:           Option<Usage>,
    /// Parent collection index.
    pub parent:          Option<usize>,
    /// Nesting depth, with top-level collections at zero.
    pub depth:           u16,
}

impl Collection {
    /// Resolves this collection's primary usage against the HUT catalog.
    #[must_use]
    pub fn usage_metadata(&self) -> Option<UsageLookup> {
        self.usage.map(lookup_usage)
    }
}

/// Preserved long item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LongItemRecord<'a> {
    /// Descriptor byte offset.
    pub offset: usize,
    /// Long-item tag.
    pub tag:    u8,
    /// Long-item payload.
    pub data:   &'a [u8],
}

/// Preserved unknown/reserved short item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownItemRecord<'a> {
    /// Descriptor byte offset.
    pub offset:    usize,
    /// Short-item type.
    pub item_type: ItemType,
    /// Four-bit tag.
    pub tag:       u8,
    /// Payload bytes.
    pub data:      &'a [u8],
}

/// Complete semantic descriptor model.
///
/// `S` selects the record buffers. The default [`VecStore`] keeps the original
/// `Vec`-backed API; [`parse_in`] can fill [`VecDequeStore`],
/// [`LinkedListStore`], or a user [`DescriptorStore`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Descriptor<'a, S: DescriptorStore<'a> = VecStore> {
    /// Original descriptor bytes.
    pub bytes:         &'a [u8],
    /// Input/Output/Feature fields in source order.
    pub fields:        S::Fields,
    /// Collections in pre-order.
    pub collections:   S::Collections,
    /// Per-kind, per-ID report lengths.
    pub reports:       S::Reports,
    /// Preserved long items.
    pub long_items:    S::LongItems,
    /// Preserved unknown short items.
    pub unknown_items: S::UnknownItems,
}

impl<'a, S: DescriptorStore<'a>> Descriptor<'a, S> {
    /// Finds one report layout.
    #[must_use]
    pub fn report(&self, kind: ReportKind, id: u8) -> Option<&ReportLayout> {
        self.reports
            .iter()
            .find(|report| report.kind == kind && report.id == id)
    }

    /// Iterates fields belonging to one report.
    pub fn report_fields(&self, kind: ReportKind, id: u8) -> impl Iterator<Item = &ReportField> + use<'_, 'a, S> {
        self.fields
            .iter()
            .filter(move |field| field.kind == kind && field.report_id == id)
    }
}

/// Lossy parse result, retaining a partial model with diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseOutput<'a, S: DescriptorStore<'a> = VecStore> {
    /// Parsed model, including everything preceding a fatal truncation.
    pub descriptor:  Descriptor<'a, S>,
    /// Semantic and structural diagnostics.
    pub diagnostics: S::Diagnostics,
}

impl<'a, S: DescriptorStore<'a>> ParseOutput<'a, S> {
    /// Returns whether any error-level diagnostic was emitted.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level == DiagnosticLevel::Error)
    }
}

/// Parses a descriptor into [`Vec`] buffers and rejects any error-level
/// diagnostics.
///
/// # Errors
///
/// Returns every collected diagnostic when the descriptor contains an
/// error-level problem.
pub fn parse(bytes: &[u8]) -> Result<Descriptor<'_>, Vec<Diagnostic>> {
    parse_with_options(bytes, ParseOptions::default())
}

/// Parses with explicit options into [`Vec`] buffers.
///
/// # Errors
///
/// Returns every collected diagnostic when the descriptor contains an
/// error-level problem.
pub fn parse_with_options(bytes: &[u8], options: ParseOptions) -> Result<Descriptor<'_>, Vec<Diagnostic>> {
    match parse_in_with_options::<VecStore>(bytes, options) {
        | Ok(descriptor) => Ok(descriptor),
        | Err(ParseFailure::Invalid(diagnostics)) => Err(diagnostics),
        | Err(ParseFailure::Store(error)) => match error {},
    }
}

/// Parses a descriptor while retaining a partial model and all diagnostics.
#[must_use]
pub fn parse_lossy(bytes: &[u8]) -> ParseOutput<'_> {
    parse_lossy_with_options(bytes, ParseOptions::default())
}

/// Lossy parser with explicit options, storing records in [`Vec`].
#[must_use]
pub fn parse_lossy_with_options(bytes: &[u8], options: ParseOptions) -> ParseOutput<'_> {
    match parse_lossy_in_with_options::<VecStore>(bytes, options) {
        | Ok(output) => output,
        | Err(error) => match error {},
    }
}

/// Parses into caller-selected record buffers.
///
/// # Errors
///
/// Returns [`ParseFailure::Invalid`] when any error-level diagnostic is
/// present, or [`ParseFailure::Store`] when a buffer rejects a record.
pub fn parse_in<'a, S: DescriptorStore<'a>>(
    bytes: &'a [u8],
) -> Result<Descriptor<'a, S>, ParseFailure<S::Diagnostics, S::Error>> {
    parse_in_with_options(bytes, ParseOptions::default())
}

/// Parses into caller-selected record buffers with explicit options.
///
/// # Errors
///
/// Returns [`ParseFailure::Invalid`] when any error-level diagnostic is
/// present, or [`ParseFailure::Store`] when a buffer rejects a record.
pub fn parse_in_with_options<'a, S: DescriptorStore<'a>>(
    bytes: &'a [u8],
    options: ParseOptions,
) -> Result<Descriptor<'a, S>, ParseFailure<S::Diagnostics, S::Error>> {
    let output = parse_lossy_in_with_options::<S>(bytes, options).map_err(ParseFailure::Store)?;
    if output.has_errors() {
        Err(ParseFailure::Invalid(output.diagnostics))
    } else {
        Ok(output.descriptor)
    }
}

/// Lossy parse into caller-selected record buffers.
///
/// # Errors
///
/// Returns the store error when a buffer rejects a record.
pub fn parse_lossy_in<'a, S: DescriptorStore<'a>>(bytes: &'a [u8]) -> Result<ParseOutput<'a, S>, S::Error> {
    parse_lossy_in_with_options(bytes, ParseOptions::default())
}

/// Lossy parse into caller-selected record buffers with explicit options.
///
/// # Errors
///
/// Returns the store error when a buffer rejects a record.
pub fn parse_lossy_in_with_options<'a, S: DescriptorStore<'a>>(
    bytes: &'a [u8],
    options: ParseOptions,
) -> Result<ParseOutput<'a, S>, S::Error> {
    Parser::<S>::new(bytes, options).run()
}

#[derive(Clone, Copy, Debug, Default)]
struct GlobalState {
    usage_page:       u16,
    logical_minimum:  i32,
    logical_maximum:  i64,
    physical_minimum: i32,
    physical_maximum: i64,
    unit_exponent:    i8,
    unit:             u32,
    report_size:      u32,
    report_count:     u32,
    report_id:        u8,
}

#[derive(Clone, Debug)]
struct LocalState {
    usage_sets:     Vec<Vec<Usage>>,
    usage_minimum:  Option<Usage>,
    usage_maximum:  Option<Usage>,
    designators:    DesignatorAssignment,
    strings:        StringAssignment,
    delimiter_open: bool,
    pending_offset: Option<usize>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            usage_sets:     vec![Vec::new()],
            usage_minimum:  None,
            usage_maximum:  None,
            designators:    DesignatorAssignment::default(),
            strings:        StringAssignment::default(),
            delimiter_open: false,
            pending_offset: None,
        }
    }
}

impl LocalState {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn usage_assignment(&self) -> UsageAssignment {
        UsageAssignment {
            sets:    self.usage_sets.clone(),
            minimum: self.usage_minimum,
            maximum: self.usage_maximum,
        }
    }

    fn mark_pending(&mut self, offset: usize) {
        if self.pending_offset.is_none() {
            self.pending_offset = Some(offset);
        }
    }

    fn push_usage(&mut self, usage: Usage) {
        let index = if self.delimiter_open {
            self.usage_sets.len() - 1
        } else {
            0
        };
        self.usage_sets[index].push(usage);
    }
}

struct Parser<'a, S: DescriptorStore<'a>> {
    bytes:                &'a [u8],
    options:              ParseOptions,
    descriptor:           Descriptor<'a, S>,
    diagnostics:          S::Diagnostics,
    store_error:          Option<S::Error>,
    global:               GlobalState,
    global_stack:         Vec<(usize, GlobalState)>,
    local:                LocalState,
    collection_stack:     Vec<usize>,
    saw_numbered_report:  bool,
    saw_unnumbered_field: bool,
}

impl<'a, S: DescriptorStore<'a>> Parser<'a, S> {
    fn new(bytes: &'a [u8], options: ParseOptions) -> Self {
        Self {
            bytes,
            options,
            descriptor: Descriptor {
                bytes,
                fields: S::Fields::default(),
                collections: S::Collections::default(),
                reports: S::Reports::default(),
                long_items: S::LongItems::default(),
                unknown_items: S::UnknownItems::default(),
            },
            diagnostics: S::Diagnostics::default(),
            store_error: None,
            global: GlobalState::default(),
            global_stack: Vec::new(),
            local: LocalState::default(),
            collection_stack: Vec::new(),
            saw_numbered_report: false,
            saw_unnumbered_field: false,
        }
    }

    fn run(mut self) -> Result<ParseOutput<'a, S>, S::Error> {
        for result in Items::new(self.bytes) {
            match result {
                | Ok(spanned) => {
                    match spanned.item {
                        | Item::Long(item) => {
                            push_record(&mut self.store_error, &mut self.descriptor.long_items, LongItemRecord {
                                offset: spanned.offset,
                                tag:    item.tag(),
                                data:   item.data(),
                            })
                        },
                        | Item::Short(item) => self.short(spanned.offset, item),
                    }
                },
                | Err(error) => {
                    let offset = match error {
                        | hidder_core::ParseError::UnexpectedEnd {
                            offset, ..
                        } => offset,
                    };
                    self.error(offset, error.to_string());
                    break;
                },
            }
        }

        if self.options.strict {
            if let Some(offset) = self.local.pending_offset {
                self.error(offset, "Local item is not followed by a main item");
            }
            if self.local.delimiter_open {
                self.error(self.bytes.len(), "Unterminated delimiter set");
            }
            let open_collections = self.collection_stack.clone();
            for index in open_collections {
                let offset = self
                    .descriptor
                    .collections
                    .get(index)
                    .map_or(0, |collection| collection.start_offset);
                self.error(offset, "Collection has no matching end collection");
            }
            let stack = self.global_stack.clone();
            for (offset, _) in stack {
                self.error(offset, "Global push has no matching pop");
            }
            if self.saw_numbered_report && self.saw_unnumbered_field {
                self.error(0, "Numbered and unnumbered report fields are mixed in one descriptor");
            }
        }

        if let Some(error) = self.store_error {
            return Err(error);
        }
        Ok(ParseOutput {
            descriptor:  self.descriptor,
            diagnostics: self.diagnostics,
        })
    }

    fn short(&mut self, offset: usize, item: ShortItem<'a>) {
        match item.tag() {
            | ItemTag::Main(tag) => self.main(offset, tag, item),
            | ItemTag::Global(tag) => self.global(offset, tag, item),
            | ItemTag::Local(tag) => self.local(offset, tag, item),
            | ItemTag::Unknown {
                item_type,
                tag,
            } => {
                push_record(
                    &mut self.store_error,
                    &mut self.descriptor.unknown_items,
                    UnknownItemRecord {
                        offset,
                        item_type,
                        tag,
                        data: item.data(),
                    },
                );
                if item_type == ItemType::Local {
                    self.local.mark_pending(offset);
                }
                if item_type == ItemType::Main {
                    if self.options.strict {
                        self.check_local_ranges(offset);
                    }
                    if self.local.delimiter_open && self.options.strict {
                        self.error(offset, "Unknown main item encountered before delimiter close");
                    }
                    self.local.reset();
                }
            },
        }
    }

    fn main(&mut self, offset: usize, tag: MainTag, item: ShortItem<'a>) {
        if self.options.strict {
            let allowed = match tag {
                | MainTag::Input | MainTag::Output | MainTag::Feature => {
                    &[DataSize::One, DataSize::Two, DataSize::Four][..]
                },
                | MainTag::Collection => &[DataSize::One][..],
                | MainTag::EndCollection => &[DataSize::Zero][..],
            };
            self.check_width(offset, item, allowed, "Main item");
            self.check_local_ranges(offset);
        }
        if self.local.delimiter_open && self.options.strict {
            self.error(offset, "Main item encountered before delimiter close");
        }
        match tag {
            | MainTag::Input => self.field(offset, ReportKind::Input, item),
            | MainTag::Output => self.field(offset, ReportKind::Output, item),
            | MainTag::Feature => self.field(offset, ReportKind::Feature, item),
            | MainTag::Collection => self.collection(offset, item),
            | MainTag::EndCollection => self.end_collection(offset, item),
        }
        self.local.reset();
    }

    fn field(&mut self, offset: usize, kind: ReportKind, item: ShortItem<'a>) {
        if self.options.strict {
            if self.global.report_size == 0 {
                self.error(offset, "Report field has zero report size");
            }
            if self.global.report_size > 32 {
                self.error(offset, "Report size exceeds the HID 1.11 32-bit field limit");
            }
            if self.global.report_count == 0 {
                self.error(offset, "Report field has zero report count");
            }
            if self.global.logical_maximum < i64::from(self.global.logical_minimum) {
                self.error(offset, "Logical maximum is less than logical minimum");
            }
        }

        let id = self.global.report_id;
        if id == 0 {
            self.saw_unnumbered_field = true;
        } else {
            self.saw_numbered_report = true;
        }
        let report_index = self.report_index(kind, id);
        let Some(report) = self.descriptor.reports.get(report_index) else {
            return;
        };
        let bit_offset = report.bit_len;
        let bit_len = u64::from(self.global.report_size).checked_mul(u64::from(self.global.report_count));
        let Some(bit_len) = bit_len else {
            self.error(offset, "Report field bit length overflows u64");
            return;
        };
        let Some(new_len) = bit_offset.checked_add(bit_len) else {
            self.error(offset, "Report length overflows u64");
            return;
        };
        let Some(field_count) = report.field_count.checked_add(1) else {
            self.error(offset, "Report field count overflows u32");
            return;
        };
        if let Some(report) = self.descriptor.reports.get_mut(report_index) {
            report.bit_len = new_len;
            report.field_count = field_count;
        }

        push_record(&mut self.store_error, &mut self.descriptor.fields, ReportField {
            offset,
            kind,
            report_id: id,
            bit_offset,
            report_size: self.global.report_size,
            report_count: self.global.report_count,
            flags: MainFlags::from_bits(item.unsigned_value()),
            logical_minimum: self.global.logical_minimum,
            logical_maximum: self.global.logical_maximum,
            physical_minimum: self.global.physical_minimum,
            physical_maximum: self.global.physical_maximum,
            unit_exponent: self.global.unit_exponent,
            unit: self.global.unit,
            usages: self.local.usage_assignment(),
            designators: self.local.designators.clone(),
            strings: self.local.strings.clone(),
            collection: self.collection_stack.last().copied(),
        });
    }

    fn collection(&mut self, offset: usize, item: ShortItem<'a>) {
        let raw = item.unsigned_value();
        if raw > u32::from(u8::MAX) {
            self.error(offset, "Collection type exceeds one byte");
        }
        let usage = self.local.usage_assignment().primary();
        if self.options.strict && usage.is_none() {
            self.error(offset, "Collection has no preceding usage");
        }
        let index = self.descriptor.collections.len();
        let depth = if let Ok(value) = u16::try_from(self.collection_stack.len()) {
            value
        } else {
            self.error(offset, "Collection nesting depth exceeds u16");
            u16::MAX
        };
        push_record(&mut self.store_error, &mut self.descriptor.collections, Collection {
            start_offset: offset,
            end_offset: None,
            collection_type: CollectionType::from_raw(raw as u8),
            usage,
            parent: self.collection_stack.last().copied(),
            depth,
        });
        self.collection_stack.push(index);
    }

    fn end_collection(&mut self, offset: usize, item: ShortItem<'a>) {
        if self.options.strict && !item.data().is_empty() {
            self.error(offset, "End collection must have a zero-byte payload");
        }
        let Some(index) = self.collection_stack.pop() else {
            self.error(offset, "End collection has no open collection");
            return;
        };
        if let Some(collection) = self.descriptor.collections.get_mut(index) {
            collection.end_offset = Some(offset);
        }
    }

    fn global(&mut self, offset: usize, tag: GlobalTag, item: ShortItem<'a>) {
        if self.options.strict {
            let allowed = match tag {
                | GlobalTag::UsagePage => &[DataSize::One, DataSize::Two][..],
                | GlobalTag::LogicalMinimum
                | GlobalTag::LogicalMaximum
                | GlobalTag::PhysicalMinimum
                | GlobalTag::PhysicalMaximum
                | GlobalTag::Unit
                | GlobalTag::ReportSize
                | GlobalTag::ReportCount => &[DataSize::One, DataSize::Two, DataSize::Four][..],
                | GlobalTag::UnitExponent | GlobalTag::ReportId => &[DataSize::One][..],
                | GlobalTag::Push | GlobalTag::Pop => &[DataSize::Zero][..],
            };
            self.check_width(offset, item, allowed, "Global item");
        }
        match tag {
            | GlobalTag::UsagePage => {
                let value = item.unsigned_value();
                if value > u32::from(u16::MAX) {
                    self.error(offset, "Usage page exceeds 16 bits");
                }
                self.global.usage_page = value as u16;
            },
            | GlobalTag::LogicalMinimum => self.global.logical_minimum = item.signed_value(),
            | GlobalTag::LogicalMaximum => {
                self.global.logical_maximum = decode_maximum(item, self.global.logical_minimum);
            },
            | GlobalTag::PhysicalMinimum => self.global.physical_minimum = item.signed_value(),
            | GlobalTag::PhysicalMaximum => {
                self.global.physical_maximum = decode_maximum(item, self.global.physical_minimum);
            },
            | GlobalTag::UnitExponent => {
                self.global.unit_exponent = decode_unit_exponent(item.unsigned_value());
                if self.options.strict && item.unsigned_value() & !0x0F != 0 {
                    self.warning(offset, "Unit exponent has non-zero bits outside the low nibble");
                }
            },
            | GlobalTag::Unit => self.global.unit = item.unsigned_value(),
            | GlobalTag::ReportSize => self.global.report_size = item.unsigned_value(),
            | GlobalTag::ReportId => {
                let value = item.unsigned_value();
                if value == 0 || value > u32::from(u8::MAX) {
                    self.error(offset, "Report ID must be in 1..=255");
                } else {
                    self.global.report_id = value as u8;
                    self.saw_numbered_report = true;
                }
            },
            | GlobalTag::ReportCount => self.global.report_count = item.unsigned_value(),
            | GlobalTag::Push => {
                if self.options.strict && !item.data().is_empty() {
                    self.error(offset, "Push must have a zero-byte payload");
                }
                self.global_stack.push((offset, self.global));
            },
            | GlobalTag::Pop => {
                if self.options.strict && !item.data().is_empty() {
                    self.error(offset, "Pop must have a zero-byte payload");
                }
                let Some((_, state)) = self.global_stack.pop() else {
                    self.error(offset, "Pop has no matching push");
                    return;
                };
                self.global = state;
            },
        }
    }

    fn local(&mut self, offset: usize, tag: LocalTag, item: ShortItem<'a>) {
        if self.options.strict {
            let allowed = match tag {
                | LocalTag::Usage
                | LocalTag::UsageMinimum
                | LocalTag::UsageMaximum
                | LocalTag::DesignatorIndex
                | LocalTag::DesignatorMinimum
                | LocalTag::DesignatorMaximum
                | LocalTag::StringIndex
                | LocalTag::StringMinimum
                | LocalTag::StringMaximum => &[DataSize::One, DataSize::Two, DataSize::Four][..],
                | LocalTag::Delimiter => &[DataSize::One][..],
            };
            self.check_width(offset, item, allowed, "Local item");
        }
        self.local.mark_pending(offset);
        match tag {
            | LocalTag::Usage => {
                let usage = self.decode_usage(item);
                self.local.push_usage(usage);
            },
            | LocalTag::UsageMinimum => {
                let usage = self.decode_usage(item);
                self.local.usage_minimum = Some(usage);
            },
            | LocalTag::UsageMaximum => {
                let usage = self.decode_usage(item);
                self.local.usage_maximum = Some(usage);
            },
            | LocalTag::DesignatorIndex => {
                self.local.designators.indices.push(item.unsigned_value());
            },
            | LocalTag::DesignatorMinimum => {
                self.local.designators.minimum = Some(item.unsigned_value());
            },
            | LocalTag::DesignatorMaximum => {
                self.local.designators.maximum = Some(item.unsigned_value());
            },
            | LocalTag::StringIndex => self.local.strings.indices.push(item.unsigned_value()),
            | LocalTag::StringMinimum => self.local.strings.minimum = Some(item.unsigned_value()),
            | LocalTag::StringMaximum => self.local.strings.maximum = Some(item.unsigned_value()),
            | LocalTag::Delimiter => {
                let value = item.unsigned_value();
                match value {
                    | 1 if self.local.delimiter_open => {
                        self.error(offset, "Delimiter sets cannot be nested");
                    },
                    | 1 => {
                        self.local.delimiter_open = true;
                        self.local.usage_sets.push(Vec::new());
                    },
                    | 0 if !self.local.delimiter_open => {
                        self.error(offset, "Delimiter close has no matching open");
                    },
                    | 0 => self.local.delimiter_open = false,
                    | _ => self.error(offset, "Delimiter value must be zero or one"),
                }
            },
        }
    }

    fn decode_usage(&self, item: ShortItem<'a>) -> Usage {
        if item.data().len() == 4 {
            Usage::from_raw(item.unsigned_value())
        } else {
            Usage::new(self.global.usage_page, item.unsigned_value() as u16)
        }
    }

    fn check_local_ranges(&mut self, offset: usize) {
        match (self.local.usage_minimum, self.local.usage_maximum) {
            | (Some(minimum), Some(maximum)) => {
                if minimum.page() != maximum.page() {
                    self.error(offset, "Usage minimum and usage maximum have different pages");
                } else if minimum.id() > maximum.id() {
                    self.error(offset, "Usage minimum exceeds usage maximum");
                }
            },
            | (Some(_), None) => self.error(offset, "Usage minimum has no usage maximum"),
            | (None, Some(_)) => self.error(offset, "Usage maximum has no usage minimum"),
            | (None, None) => {},
        }
        match (self.local.designators.minimum, self.local.designators.maximum) {
            | (Some(minimum), Some(maximum)) if minimum > maximum => {
                self.error(offset, "Designator minimum exceeds designator maximum");
            },
            | (Some(_), None) => self.error(offset, "Designator minimum has no designator maximum"),
            | (None, Some(_)) => self.error(offset, "Designator maximum has no designator minimum"),
            | _ => {},
        }
        match (self.local.strings.minimum, self.local.strings.maximum) {
            | (Some(minimum), Some(maximum)) if minimum > maximum => {
                self.error(offset, "String minimum exceeds string maximum");
            },
            | (Some(_), None) => self.error(offset, "String minimum has no string maximum"),
            | (None, Some(_)) => self.error(offset, "String maximum has no string minimum"),
            | _ => {},
        }
    }

    fn report_index(&mut self, kind: ReportKind, id: u8) -> usize {
        if let Some(index) = self
            .descriptor
            .reports
            .iter()
            .position(|report| report.kind == kind && report.id == id)
        {
            return index;
        }
        let index = self.descriptor.reports.len();
        push_record(&mut self.store_error, &mut self.descriptor.reports, ReportLayout {
            kind,
            id,
            bit_len: if id == 0 {
                0
            } else {
                8
            },
            field_count: 0,
        });
        index
    }

    fn check_width(&mut self, offset: usize, item: ShortItem<'_>, allowed: &[DataSize], family: &str) {
        if allowed.contains(&item.size()) {
            return;
        }
        self.error(
            offset,
            alloc::format!(
                "{family} tag {:#x} has invalid {}-byte payload",
                item.raw_tag(),
                item.data().len()
            ),
        );
    }

    fn error(&mut self, offset: usize, message: impl Into<String>) {
        push_record(&mut self.store_error, &mut self.diagnostics, Diagnostic {
            level: DiagnosticLevel::Error,
            offset,
            message: message.into(),
        });
    }

    fn warning(&mut self, offset: usize, message: impl Into<String>) {
        push_record(&mut self.store_error, &mut self.diagnostics, Diagnostic {
            level: DiagnosticLevel::Warning,
            offset,
            message: message.into(),
        });
    }
}

fn push_record<T, B: RecordBuf<T>>(error: &mut Option<B::Error>, buf: &mut B, value: T) {
    if error.is_some() {
        return;
    }
    if let Err(store_error) = buf.try_push(value) {
        *error = Some(store_error);
    }
}

fn decode_maximum(item: ShortItem<'_>, minimum: i32) -> i64 {
    if minimum < 0 {
        i64::from(item.signed_value())
    } else {
        i64::from(item.unsigned_value())
    }
}

#[cfg(test)]
mod tests {
    use hid_usage_tables::{
        UsageStatus,
        UsageType,
    };

    use super::*;

    const MOUSE: &[u8] = &[
        0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x15, 0x00,
        0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0x05, 0x01, 0x09, 0x30,
        0x09, 0x31, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x02, 0x81, 0x06, 0xC0, 0xC0,
    ];

    #[test]
    fn report_byte_length_rounds_up_without_overflow() {
        for (bit_len, expected) in [
            (0, 0),
            (1, 1),
            (7, 1),
            (8, 1),
            (9, 2),
            (16, 2),
            (17, 3),
            (u64::MAX, 1_u64 << 61),
        ] {
            let report = ReportLayout {
                kind: ReportKind::Input,
                id: 0,
                bit_len,
                field_count: 0,
            };
            assert_eq!(report.byte_len(), expected);
        }
    }

    #[test]
    fn builds_collection_and_report_layouts() {
        let descriptor = parse(MOUSE).unwrap();
        assert_eq!(descriptor.collections.len(), 2);
        assert_eq!(descriptor.fields.len(), 3);
        assert_eq!(descriptor.reports.len(), 1);
        assert_eq!(descriptor.reports[0].kind, ReportKind::Input);
        assert_eq!(descriptor.reports[0].bit_len, 24);
        assert_eq!(descriptor.reports[0].byte_len(), 3);
        assert_eq!(descriptor.fields[0].bit_offset, 0);
        assert_eq!(descriptor.fields[1].bit_offset, 3);
        assert_eq!(descriptor.fields[2].bit_offset, 8);
        assert_eq!(descriptor.fields[2].usages.sets[0].as_slice(), &[
            Usage::new(0x0001, 0x0030),
            Usage::new(0x0001, 0x0031)
        ]);
    }

    #[test]
    fn report_and_collection_usages_resolve_to_shared_metadata() {
        let descriptor = parse(MOUSE).unwrap();
        let UsageLookup::Defined(mouse) = descriptor.collections[0].usage_metadata().unwrap() else {
            panic!("Top-level collection should resolve to mouse");
        };
        assert_eq!(mouse.name, "Mouse");
        assert!(mouse.types.contains(UsageType::ApplicationCollection));
        assert_eq!(mouse.status, UsageStatus::Current);

        let UsageLookup::Defined(x) = descriptor.fields[2].primary_usage_metadata().unwrap() else {
            panic!("Axis field should resolve to x");
        };
        assert_eq!(x.name, "X");
    }

    #[test]
    fn preserves_long_and_unknown_short_items() {
        let bytes = [0xFE, 0x02, 0xAA, 0x10, 0x20, 0xFD, 1];
        let output = parse_lossy(&bytes);
        assert!(!output.has_errors());
        assert_eq!(output.descriptor.long_items.len(), 1);
        assert_eq!(output.descriptor.long_items[0].tag, 0xAA);
        assert_eq!(output.descriptor.long_items[0].data, [0x10, 0x20]);
        assert_eq!(output.descriptor.unknown_items.len(), 1);
        assert_eq!(output.descriptor.unknown_items[0].item_type, ItemType::Reserved);
        assert_eq!(output.descriptor.unknown_items[0].tag, 0x0F);
    }

    #[test]
    fn validates_exact_standard_item_widths() {
        let bytes = [0xA2, 0x01, 0x00, 0xC0];
        let diagnostics = parse(&bytes).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("invalid 2-byte payload"))
        );
    }

    #[test]
    fn unknown_main_items_consume_local_state() {
        let bytes = [
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x02, // Usage (Mouse)
            0x01, 0x00, // unknown Main tag 0
            0xA1, 0x01, // Collection (Application)
            0xC0,
        ];
        let output = parse_lossy(&bytes);
        assert_eq!(output.descriptor.unknown_items.len(), 1);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("Collection has no preceding usage"))
        );
    }

    #[test]
    fn diagnoses_dangling_and_unpaired_local_items() {
        let dangling = parse(&[0x09, 0x01]).unwrap_err();
        assert!(
            dangling
                .iter()
                .any(|diagnostic| diagnostic.message.contains("not followed by a main"))
        );

        let unpaired = [
            0x05, 0x09, // Usage Page (Button)
            0x19, 0x01, // Usage Minimum 1
            0x75, 0x01, // Report Size 1
            0x95, 0x01, // Report Count 1
            0x81, 0x02, // Input
        ];
        let diagnostics = parse(&unpaired).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("has no usage maximum"))
        );
    }

    #[test]
    fn numbered_reports_include_the_id_byte() {
        let bytes = [
            0x85, 0x07, // Report ID 7
            0x75, 0x08, // Report Size 8
            0x95, 0x02, // Report Count 2
            0x81, 0x02, // Input
        ];
        let descriptor = parse(&bytes).unwrap();
        let report = descriptor.report(ReportKind::Input, 7).unwrap();
        assert_eq!(report.bit_len, 24);
        assert_eq!(report.byte_len(), 3);
        assert_eq!(descriptor.fields[0].bit_offset, 8);
    }

    #[test]
    fn parse_results_fill_vecdeque_and_linked_list_stores() {
        let deque = parse_in::<VecDequeStore>(MOUSE).unwrap();
        assert_eq!(deque.collections.len(), 2);
        assert_eq!(deque.fields.len(), 3);
        assert_eq!(deque.report(ReportKind::Input, 0).unwrap().byte_len(), 3);

        let list = parse_in::<LinkedListStore>(MOUSE).unwrap();
        assert_eq!(list.collections.len(), 2);
        assert_eq!(list.fields.len(), 3);
        assert_eq!(list.report(ReportKind::Input, 0).unwrap().bit_len, 24);
    }
}
