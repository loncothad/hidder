#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Compiler for a line-oriented, HID-tool-style report descriptor language.
//!
//! The same compiler is used by the procedural macro and can be called by
//! build tools. It needs `alloc`, but not `std`; generated descriptor arrays
//! have no runtime dependency on this crate.
//!
//! ```
//! let compiled = hidder_dsl::compile(
//!     "Usage Page (Generic Desktop)\nUsage (Mouse)\nCollection (Application)\nEnd Collection",
//! )
//! .unwrap();
//! assert_eq!(compiled.bytes, [0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0xC0]);
//! ```

extern crate alloc;

use alloc::{
    format,
    string::{
        String,
        ToString,
    },
    vec::Vec,
};
use core::fmt;

use hid_usage_tables::{
    PageLookup,
    UsageLookup,
    UsageStatus,
    UsageType,
    find_page,
    find_usage,
    lookup_page,
    lookup_usage,
};
use hidder_core::{
    CollectionType,
    DataSize,
    EncodeWidth,
    EncodedShort,
    GlobalTag,
    ItemType,
    LocalTag,
    MainFlags,
    MainTag,
    Unit,
    UnitSystem,
    Usage,
    encode_short_raw,
    encode_signed,
    encode_unsigned,
};

/// Severity of a compiler diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticLevel {
    /// The descriptor cannot be emitted safely.
    Error,
    /// The descriptor is valid but suspicious or uses deprecated metadata.
    Warning,
}

/// One source diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// Error or warning.
    pub level:   DiagnosticLevel,
    /// One-based source line.
    pub line:    usize,
    /// One-based source column.
    pub column:  usize,
    /// Human-readable message.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}: {}",
            self.line,
            self.column,
            match self.level {
                | DiagnosticLevel::Error => "Error",
                | DiagnosticLevel::Warning => "Warning",
            },
            self.message
        )
    }
}

/// Controls semantic checks beyond byte-level item validity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompileOptions {
    /// Require Collection/End Collection and Push/Pop to balance.
    pub structural_checks:   bool,
    /// Require non-zero Report Size and Report Count before report fields.
    pub report_shape_checks: bool,
    /// Check collection Usage Types when source metadata is available.
    pub usage_type_lints:    bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            structural_checks:   true,
            report_shape_checks: true,
            usage_type_lints:    true,
        }
    }
}

/// Successful compilation result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Compilation {
    /// Encoded report descriptor.
    pub bytes:      Vec<u8>,
    /// Non-fatal source diagnostics.
    pub warnings:   Vec<Diagnostic>,
    /// Number of emitted HID items (raw byte blocks count as one directive).
    pub item_count: usize,
}

/// Compiles a descriptor using [`CompileOptions::default`].
///
/// # Errors
///
/// Returns all error-level diagnostics when the source cannot be compiled into
/// a structurally valid report descriptor.
pub fn compile(source: &str) -> Result<Compilation, Vec<Diagnostic>> {
    compile_with_options(source, CompileOptions::default())
}

/// Compiles a textual descriptor into bytes.
///
/// # Errors
///
/// Returns all error-level diagnostics when the source cannot be compiled into
/// a structurally valid report descriptor.
pub fn compile_with_options(source: &str, options: CompileOptions) -> Result<Compilation, Vec<Diagnostic>> {
    let mut compiler = Compiler::new(options);
    for (index, original) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comments(original).trim();
        if line.is_empty() {
            continue;
        }
        compiler.compile_line(line_number, line);
    }
    compiler.finish()
}

#[derive(Clone, Copy, Debug, Default)]
struct GlobalState {
    usage_page:       Option<u16>,
    logical_minimum:  Option<i32>,
    logical_maximum:  Option<i64>,
    physical_minimum: Option<i32>,
    physical_maximum: Option<i64>,
    unit_exponent:    i8,
    unit:             u32,
    report_size:      Option<u32>,
    report_id:        Option<u8>,
    report_count:     Option<u32>,
}

#[derive(Clone, Debug, Default)]
struct LocalState {
    usages:             Vec<Usage>,
    usage_minimum:      Option<Usage>,
    usage_maximum:      Option<Usage>,
    designator_minimum: Option<u32>,
    designator_maximum: Option<u32>,
    string_minimum:     Option<u32>,
    string_maximum:     Option<u32>,
    delimiter_open:     bool,
    pending_line:       Option<usize>,
}

impl LocalState {
    fn clear_after_main(&mut self) {
        *self = Self::default();
    }

    fn primary_usage(&self) -> Option<Usage> {
        self.usages.first().copied().or(self.usage_minimum)
    }

    fn mark_pending(&mut self, line: usize) {
        if self.pending_line.is_none() {
            self.pending_line = Some(line);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CollectionFrame {
    line: usize,
    kind: CollectionType,
}

struct Compiler {
    options:      CompileOptions,
    bytes:        Vec<u8>,
    diagnostics:  Vec<Diagnostic>,
    global:       GlobalState,
    global_stack: Vec<(usize, GlobalState)>,
    local:        LocalState,
    collections:  Vec<CollectionFrame>,
    item_count:   usize,
}

impl Compiler {
    fn new(options: CompileOptions) -> Self {
        Self {
            options,
            bytes: Vec::new(),
            diagnostics: Vec::new(),
            global: GlobalState::default(),
            global_stack: Vec::new(),
            local: LocalState::default(),
            collections: Vec::new(),
            item_count: 0,
        }
    }

    fn finish(mut self) -> Result<Compilation, Vec<Diagnostic>> {
        if self.options.structural_checks {
            if self.local.delimiter_open {
                self.error(
                    self.local.pending_line.unwrap_or(1),
                    "Delimiter set was opened but never closed",
                );
            }
            if let Some(line) = self.local.pending_line {
                self.error(line, "Local item is not followed by a main item");
            }
            if !self.collections.is_empty() {
                let frames = self.collections.clone();
                for frame in frames {
                    self.error(
                        frame.line,
                        format!("Collection opened here (type {:#04x}) was not closed", frame.kind.raw()),
                    );
                }
            }
            if !self.global_stack.is_empty() {
                let stack = self.global_stack.clone();
                for (line, _) in stack {
                    self.error(line, "Global push here has no matching pop");
                }
            }
        }
        if self.bytes.is_empty() {
            self.error(1, "Descriptor contains no items");
        }

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        for diagnostic in self.diagnostics {
            match diagnostic.level {
                | DiagnosticLevel::Error => errors.push(diagnostic),
                | DiagnosticLevel::Warning => warnings.push(diagnostic),
            }
        }
        if errors.is_empty() {
            Ok(Compilation {
                bytes: self.bytes,
                warnings,
                item_count: self.item_count,
            })
        } else {
            errors.extend(warnings);
            Err(errors)
        }
    }

    fn compile_line(&mut self, line_number: usize, line: &str) {
        let statement = match Statement::parse(line) {
            | Ok(statement) => statement,
            | Err(message) => {
                self.error(line_number, message);
                return;
            },
        };
        let name = normalize(&statement.name);
        match name.as_str() {
            | "input" => self.main_data(line_number, MainTag::Input, &statement.arguments),
            | "output" => self.main_data(line_number, MainTag::Output, &statement.arguments),
            | "feature" => self.main_data(line_number, MainTag::Feature, &statement.arguments),
            | "collection" => self.collection(line_number, &statement.arguments),
            | "endcollection" => self.end_collection(line_number, &statement.arguments),

            | "usagepage" => self.usage_page(line_number, &statement.arguments),
            | "logicalminimum" => {
                self.signed_global(
                    line_number,
                    GlobalTag::LogicalMinimum,
                    &statement.arguments,
                    SignedGlobal::LogicalMinimum,
                )
            },
            | "logicalmaximum" => {
                self.maximum_global(
                    line_number,
                    GlobalTag::LogicalMaximum,
                    &statement.arguments,
                    MaximumGlobal::Logical,
                )
            },
            | "physicalminimum" => {
                self.signed_global(
                    line_number,
                    GlobalTag::PhysicalMinimum,
                    &statement.arguments,
                    SignedGlobal::PhysicalMinimum,
                )
            },
            | "physicalmaximum" => {
                self.maximum_global(
                    line_number,
                    GlobalTag::PhysicalMaximum,
                    &statement.arguments,
                    MaximumGlobal::Physical,
                )
            },
            | "unitexponent" => self.unit_exponent(line_number, &statement.arguments),
            | "unit" => self.unit(line_number, &statement.arguments),
            | "reportsize" => {
                self.unsigned_global(
                    line_number,
                    GlobalTag::ReportSize,
                    &statement.arguments,
                    UnsignedGlobal::ReportSize,
                )
            },
            | "reportid" => {
                self.unsigned_global(
                    line_number,
                    GlobalTag::ReportId,
                    &statement.arguments,
                    UnsignedGlobal::ReportId,
                )
            },
            | "reportcount" => {
                self.unsigned_global(
                    line_number,
                    GlobalTag::ReportCount,
                    &statement.arguments,
                    UnsignedGlobal::ReportCount,
                )
            },
            | "push" => self.push(line_number, &statement.arguments),
            | "pop" => self.pop(line_number, &statement.arguments),

            | "usage" => self.local_usage(line_number, LocalTag::Usage, &statement.arguments),
            | "usageminimum" => {
                self.local_usage(line_number, LocalTag::UsageMinimum, &statement.arguments);
            },
            | "usagemaximum" => {
                self.local_usage(line_number, LocalTag::UsageMaximum, &statement.arguments);
            },
            | "designatorindex" => {
                self.unsigned_local(line_number, LocalTag::DesignatorIndex, &statement.arguments);
            },
            | "designatorminimum" => {
                self.unsigned_local(line_number, LocalTag::DesignatorMinimum, &statement.arguments)
            },
            | "designatormaximum" => {
                self.unsigned_local(line_number, LocalTag::DesignatorMaximum, &statement.arguments)
            },
            | "stringindex" => {
                self.unsigned_local(line_number, LocalTag::StringIndex, &statement.arguments);
            },
            | "stringminimum" => {
                self.unsigned_local(line_number, LocalTag::StringMinimum, &statement.arguments);
            },
            | "stringmaximum" => {
                self.unsigned_local(line_number, LocalTag::StringMaximum, &statement.arguments);
            },
            | "delimiter" => self.delimiter(line_number, &statement.arguments),

            | "rawshort" | "shortitem" => self.raw_short(line_number, &statement.arguments),
            | "longitem" | "rawlong" => self.long_item(line_number, &statement.arguments),
            | "rawbytes" | "bytes" => self.raw_bytes(line_number, &statement.arguments),
            | _ => {
                self.error(
                    line_number,
                    format!("Unknown descriptor directive `{}`", statement.name),
                )
            },
        }
    }

    fn main_data(&mut self, line: usize, tag: MainTag, args: &Arguments) {
        self.check_before_main(line);
        let (flags, width) = match parse_main_flags(args, tag) {
            | Ok(value) => value,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        if self.options.report_shape_checks {
            match self.global.report_size {
                | Some(1 ..= 32) => {},
                | Some(value) => {
                    self.error(
                        line,
                        format!("Report size must be in 1..=32 before a report field, got {value}"),
                    )
                },
                | None => self.error(line, "Report size is not set before this report field"),
            }
            match self.global.report_count {
                | Some(1 ..) => {},
                | Some(0) => self.error(line, "Report count must be non-zero"),
                | None => self.error(line, "Report count is not set before this report field"),
            }
            if let (Some(minimum), Some(maximum)) = (self.global.logical_minimum, self.global.logical_maximum) {
                if i64::from(minimum) > maximum {
                    self.error(line, "Logical minimum exceeds logical maximum");
                }
            }
        }
        self.emit_unsigned(line, ItemType::Main, tag as u8, flags.bits(), width);
        self.local.clear_after_main();
    }

    fn collection(&mut self, line: usize, args: &Arguments) {
        self.check_before_main(line);
        if let Err(message) = args.ensure_keys(&[]) {
            self.error(line, message);
            return;
        }
        let Some(value) = args.one_positional("Collection") else {
            self.error(line, "Collection requires one collection type");
            return;
        };
        let kind = match parse_collection_type(value) {
            | Ok(kind) => kind,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let usage = self.local.primary_usage();
        if usage.is_none() && self.options.structural_checks {
            self.error(line, "Collection requires a preceding usage");
        }
        if self.options.usage_type_lints {
            if let Some(usage) = usage {
                self.lint_collection_usage(line, kind, usage);
            }
        }
        self.emit_unsigned(
            line,
            ItemType::Main,
            MainTag::Collection as u8,
            u32::from(kind.raw()),
            EncodeWidth::Exact(DataSize::One),
        );
        self.collections.push(CollectionFrame {
            line,
            kind,
        });
        self.local.clear_after_main();
    }

    fn end_collection(&mut self, line: usize, args: &Arguments) {
        self.check_before_main(line);
        if !args.is_empty() {
            self.error(line, "End collection does not take arguments");
            return;
        }
        if self.collections.pop().is_none() && self.options.structural_checks {
            self.error(line, "End collection has no matching collection");
        }
        self.emit_raw(line, ItemType::Main, MainTag::EndCollection as u8, &[]);
        self.local.clear_after_main();
    }

    fn usage_page(&mut self, line: usize, args: &Arguments) {
        if let Err(message) = args.ensure_keys(&["size"]) {
            self.error(line, message);
            return;
        }
        let Some(value) = args.one_positional("Usage Page") else {
            self.error(line, "Usage page requires one page name or number");
            return;
        };
        let page = match parse_page(value) {
            | Ok(page) => page,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let width = match args.width() {
            | Ok(width) => width,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        if let Err(message) = validate_width(width, &[DataSize::One, DataSize::Two], "Usage Page") {
            self.error(line, message);
            return;
        }
        self.emit_unsigned(
            line,
            ItemType::Global,
            GlobalTag::UsagePage as u8,
            u32::from(page),
            width,
        );
        self.global.usage_page = Some(page);
        self.lint_page(line, page);
    }

    fn signed_global(&mut self, line: usize, tag: GlobalTag, args: &Arguments, target: SignedGlobal) {
        if let Err(message) = args.ensure_keys(&["size"]) {
            self.error(line, message);
            return;
        }
        let Some(value) = args.one_positional("signed Global item") else {
            self.error(line, "Item requires one signed integer");
            return;
        };
        let value = match parse_i32(value) {
            | Ok(value) => value,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let width = match args.width() {
            | Ok(width) => width,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        if let Err(message) = validate_width(
            width,
            &[DataSize::One, DataSize::Two, DataSize::Four],
            "signed Global item",
        ) {
            self.error(line, message);
            return;
        }
        self.emit_signed(line, ItemType::Global, tag as u8, value, width);
        match target {
            | SignedGlobal::LogicalMinimum => self.global.logical_minimum = Some(value),
            | SignedGlobal::PhysicalMinimum => self.global.physical_minimum = Some(value),
        }
    }

    fn maximum_global(&mut self, line: usize, tag: GlobalTag, args: &Arguments, target: MaximumGlobal) {
        if let Err(message) = args.ensure_keys(&["size"]) {
            self.error(line, message);
            return;
        }
        let Some(text) = args.one_positional("maximum Global item") else {
            self.error(line, "Item requires one integer");
            return;
        };
        let value = match parse_integer(text) {
            | Ok(value) if (-2_147_483_648 ..= 4_294_967_295).contains(&value) => value,
            | Ok(_) => {
                self.error(line, "Maximum is outside the HID 32-bit item range");
                return;
            },
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let width = match args.width() {
            | Ok(width) => width,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        if let Err(message) = validate_width(
            width,
            &[DataSize::One, DataSize::Two, DataSize::Four],
            "maximum Global item",
        ) {
            self.error(line, message);
            return;
        }
        let minimum = match target {
            | MaximumGlobal::Logical => self.global.logical_minimum,
            | MaximumGlobal::Physical => self.global.physical_minimum,
        };
        if minimum.unwrap_or(0) < 0 {
            let Ok(signed) = i32::try_from(value) else {
                self.error(
                    line,
                    "Maximum must fit i32 because the corresponding minimum is negative",
                );
                return;
            };
            self.emit_signed(line, ItemType::Global, tag as u8, signed, width);
        } else {
            let Ok(unsigned) = u32::try_from(value) else {
                self.error(
                    line,
                    "Maximum must be non-negative because the corresponding minimum is not negative",
                );
                return;
            };
            self.emit_unsigned(line, ItemType::Global, tag as u8, unsigned, width);
        }
        match target {
            | MaximumGlobal::Logical => self.global.logical_maximum = Some(value),
            | MaximumGlobal::Physical => self.global.physical_maximum = Some(value),
        }
    }

    fn unsigned_global(&mut self, line: usize, tag: GlobalTag, args: &Arguments, target: UnsignedGlobal) {
        if let Err(message) = args.ensure_keys(&["size"]) {
            self.error(line, message);
            return;
        }
        let Some(text) = args.one_positional("unsigned Global item") else {
            self.error(line, "Item requires one unsigned integer");
            return;
        };
        let value = match parse_u32(text) {
            | Ok(value) => value,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        match target {
            | UnsignedGlobal::ReportSize if value == 0 => {
                self.error(line, "Report size must be non-zero");
            },
            | UnsignedGlobal::ReportSize if value > 32 => {
                self.error(line, "Report size exceeds the HID 1.11 32-bit field limit");
            },
            | UnsignedGlobal::ReportCount if value == 0 => {
                self.error(line, "Report count must be non-zero");
            },
            | UnsignedGlobal::ReportId if !(1 ..= 255).contains(&value) => {
                self.error(line, "Report ID must be in 1..=255; zero is reserved");
            },
            | _ => {},
        }
        let width = match args.width() {
            | Ok(width) => width,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let allowed = match target {
            | UnsignedGlobal::ReportId => &[DataSize::One][..],
            | UnsignedGlobal::ReportSize | UnsignedGlobal::ReportCount => {
                &[DataSize::One, DataSize::Two, DataSize::Four][..]
            },
        };
        if let Err(message) = validate_width(width, allowed, "unsigned Global item") {
            self.error(line, message);
            return;
        }
        self.emit_unsigned(line, ItemType::Global, tag as u8, value, width);
        match target {
            | UnsignedGlobal::ReportSize => self.global.report_size = Some(value),
            | UnsignedGlobal::ReportCount => self.global.report_count = Some(value),
            | UnsignedGlobal::ReportId => {
                self.global.report_id = u8::try_from(value).ok().filter(|id| *id != 0);
            },
        }
    }

    fn unit_exponent(&mut self, line: usize, args: &Arguments) {
        if let Err(message) = args.ensure_keys(&[]) {
            self.error(line, message);
            return;
        }
        let Some(text) = args.one_positional("Unit Exponent") else {
            self.error(line, "Unit exponent requires one value in -8..=7");
            return;
        };
        let value = match parse_integer(text) {
            | Ok(value @ -8 ..= 7) => value as i8,
            | Ok(_) => {
                self.error(line, "Unit exponent must fit the signed four-bit range -8..=7");
                return;
            },
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let byte = (value as u8) & 0x0F;
        self.emit_raw(line, ItemType::Global, GlobalTag::UnitExponent as u8, &[byte]);
        self.global.unit_exponent = value;
    }

    fn unit(&mut self, line: usize, args: &Arguments) {
        if let Err(message) = args.ensure_keys(&[
            "size",
            "length",
            "mass",
            "time",
            "temperature",
            "current",
            "luminousintensity",
        ]) {
            self.error(line, message);
            return;
        }
        let numeric_form =
            args.positional.len() == 1 && args.key_values.iter().all(|(key, _)| normalize(key) == "size");
        let raw = if numeric_form {
            match parse_u32(&args.positional[0]) {
                | Ok(value) => value,
                | Err(_) => {
                    match parse_unit_parts(args) {
                        | Ok(unit) => unit.raw(),
                        | Err(message) => {
                            self.error(line, message);
                            return;
                        },
                    }
                },
            }
        } else {
            match parse_unit_parts(args) {
                | Ok(unit) => unit.raw(),
                | Err(message) => {
                    self.error(line, message);
                    return;
                },
            }
        };
        let width = match args.width() {
            | Ok(width) => width,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        if let Err(message) = validate_width(width, &[DataSize::One, DataSize::Two, DataSize::Four], "Unit") {
            self.error(line, message);
            return;
        }
        self.emit_unsigned(line, ItemType::Global, GlobalTag::Unit as u8, raw, width);
        self.global.unit = raw;
    }

    fn push(&mut self, line: usize, args: &Arguments) {
        if !args.is_empty() {
            self.error(line, "Push does not take arguments");
            return;
        }
        self.emit_raw(line, ItemType::Global, GlobalTag::Push as u8, &[]);
        self.global_stack.push((line, self.global));
    }

    fn pop(&mut self, line: usize, args: &Arguments) {
        if !args.is_empty() {
            self.error(line, "Pop does not take arguments");
            return;
        }
        let Some((_, state)) = self.global_stack.pop() else {
            self.error(line, "Pop has no matching push");
            return;
        };
        self.emit_raw(line, ItemType::Global, GlobalTag::Pop as u8, &[]);
        self.global = state;
    }

    fn local_usage(&mut self, line: usize, tag: LocalTag, args: &Arguments) {
        if let Err(message) = args.ensure_keys(&["size"]) {
            self.error(line, message);
            return;
        }
        let Some(text) = args.one_positional("Usage") else {
            self.error(line, "Usage item requires one name or numeric identifier");
            return;
        };
        let (usage, extended) = match parse_usage(text, self.global.usage_page) {
            | Ok(value) => value,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let mut width = match args.width() {
            | Ok(width) => width,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        if let Err(message) = validate_width(width, &[DataSize::One, DataSize::Two, DataSize::Four], "Usage") {
            self.error(line, message);
            return;
        }
        let encoded_value = if extended {
            usage.raw()
        } else {
            u32::from(usage.id())
        };
        if extended {
            match width {
                | EncodeWidth::Auto => width = EncodeWidth::Exact(DataSize::Four),
                | EncodeWidth::Exact(DataSize::Four) => {},
                | EncodeWidth::Exact(_) => {
                    self.error(line, "Page-qualified/extended usage requires size=4");
                    return;
                },
            }
        }
        self.emit_unsigned(line, ItemType::Local, tag as u8, encoded_value, width);
        self.local.mark_pending(line);
        match tag {
            | LocalTag::Usage => self.local.usages.push(usage),
            | LocalTag::UsageMinimum => self.local.usage_minimum = Some(usage),
            | LocalTag::UsageMaximum => self.local.usage_maximum = Some(usage),
            | _ => {},
        }
        if let (Some(minimum), Some(maximum)) = (self.local.usage_minimum, self.local.usage_maximum) {
            if minimum.page() != maximum.page() {
                self.error(line, "Usage minimum and usage maximum use different pages");
            } else if minimum.id() > maximum.id() {
                self.error(line, "Usage minimum exceeds usage maximum");
            }
        }
        self.lint_usage(line, usage);
    }

    fn unsigned_local(&mut self, line: usize, tag: LocalTag, args: &Arguments) {
        if let Err(message) = args.ensure_keys(&["size"]) {
            self.error(line, message);
            return;
        }
        let Some(text) = args.one_positional("Local item") else {
            self.error(line, "Item requires one unsigned integer");
            return;
        };
        let value = match parse_u32(text) {
            | Ok(value) => value,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let width = match args.width() {
            | Ok(width) => width,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        if let Err(message) = validate_width(width, &[DataSize::One, DataSize::Two, DataSize::Four], "Local item") {
            self.error(line, message);
            return;
        }
        self.emit_unsigned(line, ItemType::Local, tag as u8, value, width);
        self.local.mark_pending(line);
        match tag {
            | LocalTag::DesignatorMinimum => self.local.designator_minimum = Some(value),
            | LocalTag::DesignatorMaximum => self.local.designator_maximum = Some(value),
            | LocalTag::StringMinimum => self.local.string_minimum = Some(value),
            | LocalTag::StringMaximum => self.local.string_maximum = Some(value),
            | _ => {},
        }
        self.check_local_range_order(line);
    }

    fn delimiter(&mut self, line: usize, args: &Arguments) {
        if let Err(message) = args.ensure_keys(&[]) {
            self.error(line, message);
            return;
        }
        let Some(text) = args.one_positional("Delimiter") else {
            self.error(line, "Delimiter requires open/close or 1/0");
            return;
        };
        let normalized = normalize(text);
        let value = match normalized.as_str() {
            | "open" | "1" => 1_u8,
            | "close" | "0" => 0_u8,
            | _ => {
                self.error(line, "Delimiter must be open, close, 1, or 0");
                return;
            },
        };
        if value == 1 {
            if self.local.delimiter_open {
                self.error(line, "Delimiter sets cannot be nested");
            }
            self.local.delimiter_open = true;
        } else if !self.local.delimiter_open {
            self.error(line, "Delimiter close has no matching open");
        } else {
            self.local.delimiter_open = false;
        }
        self.emit_unsigned(
            line,
            ItemType::Local,
            LocalTag::Delimiter as u8,
            u32::from(value),
            EncodeWidth::Exact(DataSize::One),
        );
        self.local.mark_pending(line);
    }

    fn raw_short(&mut self, line: usize, args: &Arguments) {
        if let Err(message) = args.ensure_keys(&["type", "tag", "data"]) {
            self.error(line, message);
            return;
        }
        if !args.positional.is_empty() {
            self.error(line, "Raw short accepts only `type=`, `tag=`, and optional `data=`");
            return;
        }
        let item_type = match args.required("type") {
            | Ok(value) => {
                match parse_item_type(value) {
                    | Ok(value) => value,
                    | Err(message) => {
                        self.error(line, message);
                        return;
                    },
                }
            },
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let tag = match args.required("tag").and_then(parse_u8) {
            | Ok(value @ 0 ..= 0x0F) => value,
            | Ok(_) => {
                self.error(line, "Raw short tag must be in 0..=15");
                return;
            },
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let data = match args.optional("data") {
            | Some(value) => {
                match parse_byte_list(value) {
                    | Ok(value) => value,
                    | Err(message) => {
                        self.error(line, message);
                        return;
                    },
                }
            },
            | None => Vec::new(),
        };
        if item_type == ItemType::Main {
            self.check_before_main(line);
        }
        match encode_short_raw(item_type, tag, &data) {
            | Ok(item) => {
                self.append(item);
                if item_type == ItemType::Main {
                    self.local.clear_after_main();
                } else if item_type == ItemType::Local {
                    self.local.mark_pending(line);
                }
            },
            | Err(error) => self.error(line, error.to_string()),
        }
    }

    fn long_item(&mut self, line: usize, args: &Arguments) {
        if let Err(message) = args.ensure_keys(&["tag", "data"]) {
            self.error(line, message);
            return;
        }
        if !args.positional.is_empty() {
            self.error(line, "Long item accepts only `tag=` and `data=`");
            return;
        }
        let tag = match args.required("tag").and_then(parse_u8) {
            | Ok(value) => value,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        let data = match args.required("data").and_then(parse_byte_list) {
            | Ok(value) => value,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        if data.len() > usize::from(u8::MAX) {
            self.error(line, "Long item payload exceeds 255 bytes");
            return;
        }
        self.bytes.push(0xFE);
        self.bytes.push(data.len() as u8);
        self.bytes.push(tag);
        self.bytes.extend_from_slice(&data);
        self.item_count += 1;
    }

    fn raw_bytes(&mut self, line: usize, args: &Arguments) {
        if let Err(message) = args.ensure_keys(&["data"]) {
            self.error(line, message);
            return;
        }
        if args.positional.len() > 1 || (!args.positional.is_empty() && args.optional("data").is_some()) {
            self.error(
                line,
                "Raw bytes accepts either one positional byte list or `data=`, not both",
            );
            return;
        }
        let source = if args.positional.len() == 1 {
            &args.positional[0]
        } else if let Some(value) = args.optional("data") {
            value
        } else {
            self.error(line, "Raw bytes requires one byte list");
            return;
        };
        let data = match parse_byte_list(source) {
            | Ok(value) => value,
            | Err(message) => {
                self.error(line, message);
                return;
            },
        };
        if data.is_empty() {
            self.error(line, "Raw bytes cannot be empty");
            return;
        }
        self.warning(
            line,
            "Raw bytes bypass item/state validation; prefer raw short or long item",
        );
        self.bytes.extend_from_slice(&data);
        self.item_count += 1;
    }

    fn emit_raw(&mut self, line: usize, item_type: ItemType, tag: u8, data: &[u8]) {
        match encode_short_raw(item_type, tag, data) {
            | Ok(item) => self.append(item),
            | Err(error) => self.error(line, error.to_string()),
        }
    }

    fn emit_unsigned(&mut self, line: usize, item_type: ItemType, tag: u8, value: u32, width: EncodeWidth) {
        match encode_unsigned(item_type, tag, value, width) {
            | Ok(item) => self.append(item),
            | Err(error) => self.error(line, error.to_string()),
        }
    }

    fn emit_signed(&mut self, line: usize, item_type: ItemType, tag: u8, value: i32, width: EncodeWidth) {
        match encode_signed(item_type, tag, value, width) {
            | Ok(item) => self.append(item),
            | Err(error) => self.error(line, error.to_string()),
        }
    }

    fn append(&mut self, item: EncodedShort) {
        self.bytes.extend_from_slice(item.as_slice());
        self.item_count += 1;
    }

    fn check_before_main(&mut self, line: usize) {
        if !self.options.structural_checks {
            return;
        }
        if self.local.delimiter_open {
            self.error(line, "Main item encountered before delimiter close");
        }
        match (self.local.usage_minimum, self.local.usage_maximum) {
            | (Some(_), None) => self.error(line, "Usage minimum has no usage maximum"),
            | (None, Some(_)) => self.error(line, "Usage maximum has no usage minimum"),
            | _ => {},
        }
        match (self.local.designator_minimum, self.local.designator_maximum) {
            | (Some(_), None) => self.error(line, "Designator minimum has no designator maximum"),
            | (None, Some(_)) => self.error(line, "Designator maximum has no designator minimum"),
            | _ => {},
        }
        match (self.local.string_minimum, self.local.string_maximum) {
            | (Some(_), None) => self.error(line, "String minimum has no string maximum"),
            | (None, Some(_)) => self.error(line, "String maximum has no string minimum"),
            | _ => {},
        }
    }

    fn check_local_range_order(&mut self, line: usize) {
        if let (Some(minimum), Some(maximum)) = (self.local.designator_minimum, self.local.designator_maximum) {
            if minimum > maximum {
                self.error(line, "Designator minimum exceeds designator maximum");
            }
        }
        if let (Some(minimum), Some(maximum)) = (self.local.string_minimum, self.local.string_maximum) {
            if minimum > maximum {
                self.error(line, "String minimum exceeds string maximum");
            }
        }
    }

    fn lint_page(&mut self, line: usize, page: u16) {
        match lookup_page(page) {
            | PageLookup::Reserved(range) => {
                self.warning(
                    line,
                    format!(
                        "Usage page {page:#06x} is reserved in HUT {version} ({:#06x}..={:#06x})",
                        range.range.start,
                        range.range.end,
                        version = hid_usage_tables::HUT_VERSION
                    ),
                )
            },
            | PageLookup::VendorDefined(_) | PageLookup::Page(_) => {},
            | PageLookup::Unknown(_) => {
                self.warning(
                    line,
                    format!(
                        "Usage page {page:#06x} is not classified by HUT {}",
                        hid_usage_tables::HUT_VERSION
                    ),
                )
            },
        }
    }

    fn lint_usage(&mut self, line: usize, usage: Usage) {
        match lookup_usage(usage) {
            | UsageLookup::Defined(entry) if entry.status == UsageStatus::Deprecated => {
                self.warning(
                    line,
                    format!(
                        "Usage `{}` ({:#06x}:{:#06x}) is deprecated",
                        entry.name, entry.page, entry.id
                    ),
                )
            },
            | UsageLookup::Reserved(range) => {
                self.warning(
                    line,
                    format!(
                        "Usage {:#06x}:{:#06x} lies in reserved range {:#06x}..={:#06x}",
                        usage.page(),
                        usage.id(),
                        range.range.start,
                        range.range.end
                    ),
                )
            },
            | _ => {},
        }
    }

    fn lint_collection_usage(&mut self, line: usize, kind: CollectionType, usage: Usage) {
        let UsageLookup::Defined(entry) = lookup_usage(usage) else {
            return;
        };
        let expected = match kind.raw() {
            | 0x00 => Some(UsageType::PhysicalCollection),
            | 0x01 => Some(UsageType::ApplicationCollection),
            | 0x02 => Some(UsageType::LogicalCollection),
            | 0x04 => Some(UsageType::NamedArray),
            | 0x05 => Some(UsageType::UsageSwitch),
            | 0x06 => Some(UsageType::UsageModifier),
            | _ => None,
        };
        if let Some(expected) = expected {
            if !entry.types.contains(expected) {
                self.warning(
                    line,
                    format!(
                        "Usage `{}` has types {:?}, not the usual {} for collection type {:#04x}",
                        entry.name,
                        entry.types,
                        expected.abbreviation(),
                        kind.raw()
                    ),
                );
            }
        }
    }

    fn error(&mut self, line: usize, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Error,
            line,
            column: 1,
            message: message.into(),
        });
    }

    fn warning(&mut self, line: usize, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            line,
            column: 1,
            message: message.into(),
        });
    }
}

#[derive(Clone, Copy)]
enum SignedGlobal {
    LogicalMinimum,
    PhysicalMinimum,
}

#[derive(Clone, Copy)]
enum MaximumGlobal {
    Logical,
    Physical,
}

#[derive(Clone, Copy)]
enum UnsignedGlobal {
    ReportSize,
    ReportId,
    ReportCount,
}

#[derive(Clone, Debug, Default)]
struct Arguments {
    positional: Vec<String>,
    key_values: Vec<(String, String)>,
}

impl Arguments {
    fn is_empty(&self) -> bool {
        self.positional.is_empty() && self.key_values.is_empty()
    }

    fn one_positional(&self, _item: &str) -> Option<&str> {
        if self.positional.len() == 1 {
            Some(self.positional[0].as_str())
        } else {
            None
        }
    }

    fn optional(&self, key: &str) -> Option<&str> {
        let normalized = normalize(key);
        self.key_values
            .iter()
            .find(|(candidate, _)| normalize(candidate) == normalized)
            .map(|(_, value)| value.as_str())
    }

    fn required(&self, key: &str) -> Result<&str, String> {
        self.optional(key)
            .ok_or_else(|| format!("Missing required `{key}=` argument"))
    }

    fn ensure_keys(&self, allowed: &[&str]) -> Result<(), String> {
        for (key, _) in &self.key_values {
            let normalized = normalize(key);
            if !allowed.iter().any(|allowed_key| normalize(allowed_key) == normalized) {
                let allowed_text = if allowed.is_empty() {
                    "no named arguments".to_string()
                } else {
                    format!("Only {}", allowed.join(", "))
                };
                return Err(format!(
                    "Unknown named argument `{key}=`; this directive accepts {allowed_text}"
                ));
            }
        }
        Ok(())
    }

    fn width(&self) -> Result<EncodeWidth, String> {
        let Some(value) = self.optional("size") else {
            return Ok(EncodeWidth::Auto);
        };
        let size = match parse_u8(value)? {
            | 0 => DataSize::Zero,
            | 1 => DataSize::One,
            | 2 => DataSize::Two,
            | 4 => DataSize::Four,
            | other => return Err(format!("Size={other} is invalid; use 0, 1, 2, or 4")),
        };
        Ok(EncodeWidth::Exact(size))
    }
}

#[derive(Clone, Debug)]
struct Statement {
    name:      String,
    arguments: Arguments,
}

impl Statement {
    fn parse(line: &str) -> Result<Self, String> {
        let line = line.trim().trim_end_matches(';').trim();
        if let Some(open) = line.find('(') {
            let close = line.rfind(')').ok_or_else(|| "Missing closing `)`".to_string())?;
            if close < open || !line[close + 1 ..].trim().is_empty() {
                return Err("Unexpected text after closing `)`".to_string());
            }
            let name = line[.. open].trim();
            if name.is_empty() {
                return Err("Missing directive name".to_string());
            }
            let arguments = parse_arguments(&line[open + 1 .. close])?;
            Ok(Self {
                name: name.to_string(),
                arguments,
            })
        } else {
            if line.contains(')') {
                return Err("Unexpected `)`".to_string());
            }
            Ok(Self {
                name:      line.to_string(),
                arguments: Arguments::default(),
            })
        }
    }
}

fn parse_arguments(source: &str) -> Result<Arguments, String> {
    let mut result = Arguments::default();
    for argument in split_top_level(source, ',')? {
        let argument = argument.trim();
        if argument.is_empty() {
            continue;
        }
        if let Some(index) = find_top_level(argument, '=') {
            let key = argument[.. index].trim();
            let value = argument[index + 1 ..].trim();
            if key.is_empty() || value.is_empty() {
                return Err(format!("Invalid key/value argument `{argument}`"));
            }
            let normalized = normalize(key);
            if result
                .key_values
                .iter()
                .any(|(candidate, _)| normalize(candidate) == normalized)
            {
                return Err(format!("Duplicate named argument `{key}=`"));
            }
            result.key_values.push((key.to_string(), unquote(value)?));
        } else {
            result.positional.push(unquote(argument)?);
        }
    }
    Ok(result)
}

fn strip_comments(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quoted = false;
    let mut escaped = false;
    let mut index = 0_usize;
    while index + 1 < bytes.len() {
        let byte = bytes[index];
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
        } else if byte == b'/' && bytes[index + 1] == b'/' {
            return &line[.. index];
        }
        index += 1;
    }
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        ""
    } else {
        line
    }
}

fn split_top_level(source: &str, delimiter: char) -> Result<Vec<&str>, String> {
    let mut result = Vec::new();
    let mut bracket_depth = 0_u32;
    let mut paren_depth = 0_u32;
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0_usize;
    for (index, character) in source.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            | '"' => quoted = true,
            | '[' => bracket_depth += 1,
            | ']' => {
                bracket_depth = bracket_depth
                    .checked_sub(1)
                    .ok_or_else(|| "Unmatched `]`".to_string())?;
            },
            | '(' => paren_depth += 1,
            | ')' => {
                paren_depth = paren_depth
                    .checked_sub(1)
                    .ok_or_else(|| "Unmatched `)` in arguments".to_string())?;
            },
            | value if value == delimiter && bracket_depth == 0 && paren_depth == 0 => {
                result.push(&source[start .. index]);
                start = index + value.len_utf8();
            },
            | _ => {},
        }
    }
    if quoted {
        return Err("Unterminated quoted string".to_string());
    }
    if bracket_depth != 0 || paren_depth != 0 {
        return Err("Unbalanced brackets or parentheses".to_string());
    }
    result.push(&source[start ..]);
    Ok(result)
}

fn find_top_level(source: &str, target: char) -> Option<usize> {
    let mut bracket_depth = 0_u32;
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in source.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            | '"' => quoted = true,
            | '[' => bracket_depth += 1,
            | ']' => bracket_depth = bracket_depth.saturating_sub(1),
            | value if value == target && bracket_depth == 0 => return Some(index),
            | _ => {},
        }
    }
    None
}

fn unquote(value: &str) -> Result<String, String> {
    let value = value.trim();
    if !value.starts_with('"') {
        return Ok(value.to_string());
    }
    if !value.ends_with('"') || value.len() < 2 {
        return Err("Unterminated quoted string".to_string());
    }
    let body = &value[1 .. value.len() - 1];
    let mut output = String::new();
    let mut characters = body.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let escaped = characters
            .next()
            .ok_or_else(|| "Trailing backslash in quoted string".to_string())?;
        match escaped {
            | '\\' => output.push('\\'),
            | '"' => output.push('"'),
            | 'n' => output.push('\n'),
            | 'r' => output.push('\r'),
            | 't' => output.push('\t'),
            | other => return Err(format!("Unsupported string escape `\\{other}`")),
        }
    }
    Ok(output)
}

fn normalize(value: &str) -> String {
    value
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect()
}

fn parse_integer(source: &str) -> Result<i64, String> {
    let source = source.trim().replace('_', "");
    if source.is_empty() {
        return Err("Expected integer".to_string());
    }
    let (negative, body) = if let Some(body) = source.strip_prefix('-') {
        (true, body)
    } else if let Some(body) = source.strip_prefix('+') {
        (false, body)
    } else {
        (false, source.as_str())
    };
    let (radix, digits) = if let Some(digits) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        (16, digits)
    } else if let Some(digits) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
        (2, digits)
    } else if let Some(digits) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
        (8, digits)
    } else {
        (10, body)
    };
    if digits.is_empty() {
        return Err(format!("Invalid integer `{source}`"));
    }
    let magnitude = i64::from_str_radix(digits, radix).map_err(|_| format!("Invalid integer `{source}`"))?;
    if negative {
        magnitude
            .checked_neg()
            .ok_or_else(|| format!("Integer `{source}` overflows i64"))
    } else {
        Ok(magnitude)
    }
}

fn parse_i32(source: &str) -> Result<i32, String> {
    i32::try_from(parse_integer(source)?).map_err(|_| format!("`{source}` Does not fit i32"))
}

fn parse_u32(source: &str) -> Result<u32, String> {
    u32::try_from(parse_integer(source)?).map_err(|_| format!("`{source}` Does not fit u32"))
}

fn parse_u16(source: &str) -> Result<u16, String> {
    u16::try_from(parse_integer(source)?).map_err(|_| format!("`{source}` Does not fit u16"))
}

fn parse_u8(source: &str) -> Result<u8, String> {
    u8::try_from(parse_integer(source)?).map_err(|_| format!("`{source}` Does not fit u8"))
}

fn parse_page(source: &str) -> Result<u16, String> {
    if let Ok(value) = parse_u16(source) {
        return Ok(value);
    }
    find_page(source)
        .map(|page| page.id)
        .ok_or_else(|| format!("Unknown usage page `{source}` in HUT {}", hid_usage_tables::HUT_VERSION))
}

fn parse_usage(source: &str, current_page: Option<u16>) -> Result<(Usage, bool), String> {
    if let Some(index) = find_top_level(source, ':') {
        let page_text = source[.. index].trim();
        let usage_text = source[index + 1 ..].trim();
        let page = parse_page(page_text)?;
        let id = match parse_u16(usage_text) {
            | Ok(id) => id,
            | Err(_) => {
                find_usage(page, usage_text)
                    .map(Usage::id)
                    .ok_or_else(|| format!("Unknown usage `{usage_text}` on page `{page_text}`"))?
            },
        };
        return Ok((Usage::new(page, id), true));
    }
    if let Ok(raw) = parse_u32(source) {
        if raw > u32::from(u16::MAX) {
            return Ok((Usage::from_raw(raw), true));
        }
        return Ok((Usage::new(current_page.unwrap_or(0), raw as u16), false));
    }
    let page = current_page
        .ok_or_else(|| format!("Named usage `{source}` requires a preceding usage page or `page: usage` syntax"))?;
    find_usage(page, source)
        .map(|usage| (usage, false))
        .ok_or_else(|| format!("Unknown usage `{source}` on page {page:#06x}"))
}

fn parse_collection_type(source: &str) -> Result<CollectionType, String> {
    if let Ok(raw) = parse_u8(source) {
        return Ok(CollectionType::from_raw(raw));
    }
    let value = match normalize(source).as_str() {
        | "physical" => CollectionType::PHYSICAL,
        | "application" => CollectionType::APPLICATION,
        | "logical" => CollectionType::LOGICAL,
        | "report" => CollectionType::REPORT,
        | "namedarray" => CollectionType::NAMED_ARRAY,
        | "usageswitch" => CollectionType::USAGE_SWITCH,
        | "usagemodifier" => CollectionType::USAGE_MODIFIER,
        | _ => return Err(format!("Unknown collection type `{source}`")),
    };
    Ok(value)
}

fn parse_item_type(source: &str) -> Result<ItemType, String> {
    if let Ok(value) = parse_u8(source) {
        return match value {
            | 0 => Ok(ItemType::Main),
            | 1 => Ok(ItemType::Global),
            | 2 => Ok(ItemType::Local),
            | 3 => Ok(ItemType::Reserved),
            | _ => Err("Item type must be 0..=3".to_string()),
        };
    }
    match normalize(source).as_str() {
        | "main" => Ok(ItemType::Main),
        | "global" => Ok(ItemType::Global),
        | "local" => Ok(ItemType::Local),
        | "reserved" => Ok(ItemType::Reserved),
        | _ => Err(format!("Unknown short-item type `{source}`")),
    }
}

fn parse_main_flags(args: &Arguments, tag: MainTag) -> Result<(MainFlags, EncodeWidth), String> {
    args.ensure_keys(&["bits", "size"])?;
    let width = args.width()?;
    validate_width(width, &[DataSize::One, DataSize::Two, DataSize::Four], "Main item")?;
    let non_size_keys = args
        .key_values
        .iter()
        .filter(|(key, _)| normalize(key) != "size")
        .count();
    if non_size_keys != 0 {
        if let Some(bits) = args.optional("bits") {
            if non_size_keys == 1 && args.positional.is_empty() {
                return Ok((MainFlags::from_bits(parse_u32(bits)?), width));
            }
        }
        return Err(
            "Main item flags accept names, one numeric value, or `bits=...`, plus optional `size=`".to_string(),
        );
    }
    if args.positional.len() == 1 {
        if let Ok(bits) = parse_u32(&args.positional[0]) {
            return Ok((MainFlags::from_bits(bits), width));
        }
    }
    let mut bits = 0_u32;
    let mut categories = [None::<String>, None, None, None, None, None, None, None, None];
    for flag in &args.positional {
        let normalized = normalize(flag);
        let (category, set) = match normalized.as_str() {
            | "data" => (0, 0),
            | "constant" => (0, MainFlags::CONSTANT),
            | "array" => (1, 0),
            | "variable" => (1, MainFlags::VARIABLE),
            | "absolute" => (2, 0),
            | "relative" => (2, MainFlags::RELATIVE),
            | "nowrap" => (3, 0),
            | "wrap" => (3, MainFlags::WRAP),
            | "linear" => (4, 0),
            | "nonlinear" => (4, MainFlags::NON_LINEAR),
            | "preferredstate" | "preferred" => (5, 0),
            | "nopreferredstate" | "nopreferred" => (5, MainFlags::NO_PREFERRED),
            | "nonullposition" | "nonull" => (6, 0),
            | "nullstate" | "null" => (6, MainFlags::NULL_STATE),
            | "nonvolatile" => (7, 0),
            | "volatile" => {
                if tag == MainTag::Input {
                    return Err("Volatile is reserved for input items".to_string());
                }
                (7, MainFlags::VOLATILE)
            },
            | "bitfield" => (8, 0),
            | "bufferedbytes" => (8, MainFlags::BUFFERED_BYTES),
            | _ => return Err(format!("Unknown main item flag `{flag}`")),
        };
        if let Some(previous) = &categories[category] {
            return Err(format!("Conflicting flag category: `{previous}` and `{flag}`"));
        }
        categories[category] = Some(flag.clone());
        bits |= set;
    }
    Ok((MainFlags::from_bits(bits), width))
}

fn validate_width(width: EncodeWidth, allowed: &[DataSize], item: &str) -> Result<(), String> {
    let EncodeWidth::Exact(size) = width else {
        return Ok(());
    };
    if allowed.contains(&size) {
        return Ok(());
    }
    let mut values = String::new();
    for (index, value) in allowed.iter().enumerate() {
        if index != 0 {
            values.push_str(", ");
        }
        values.push_str(&value.bytes().to_string());
    }
    Err(format!(
        "{item} cannot use size={}; allowed payload widths are {values} byte(s)",
        size.bytes()
    ))
}

fn parse_unit_parts(args: &Arguments) -> Result<Unit, String> {
    let system_text = args
        .positional
        .first()
        .ok_or_else(|| "Unit requires a numeric value or unit system name".to_string())?;
    if args.positional.len() > 1 {
        return Err("Unit accepts one system name followed by exponent key/value arguments".to_string());
    }
    let system = match normalize(system_text).as_str() {
        | "none" => UnitSystem::None,
        | "silinear" => UnitSystem::SiLinear,
        | "sirotation" => UnitSystem::SiRotation,
        | "englishlinear" => UnitSystem::EnglishLinear,
        | "englishrotation" => UnitSystem::EnglishRotation,
        | _ => return Err(format!("Unknown unit system `{system_text}`")),
    };
    let exponent = |name: &str| -> Result<i8, String> {
        match args.optional(name) {
            | Some(value) => {
                let value = parse_integer(value)?;
                i8::try_from(value).map_err(|_| format!("Unit exponent `{name}` does not fit i8"))
            },
            | None => Ok(0),
        }
    };
    Unit::new(
        system,
        exponent("length")?,
        exponent("mass")?,
        exponent("time")?,
        exponent("temperature")?,
        exponent("current")?,
        exponent("luminousintensity")?,
    )
    .map_err(|error| format!("Unit exponent {} is outside -8..=7", error.0))
}

fn parse_byte_list(source: &str) -> Result<Vec<u8>, String> {
    let source = source.trim();
    if !source.starts_with('[') || !source.ends_with(']') {
        return Err("Byte data must use `[0x01, 0x02, ...]` syntax".to_string());
    }
    let body = &source[1 .. source.len() - 1];
    let mut result = Vec::new();
    for value in split_top_level(body, ',')? {
        if value.trim().is_empty() {
            continue;
        }
        result.push(parse_u8(value.trim())?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUSE_SOURCE: &str = r"
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

    #[test]
    fn compiles_reference_mouse_bytes() {
        let compilation = compile(MOUSE_SOURCE).unwrap();
        assert_eq!(compilation.bytes, [
            0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x15, 0x00,
            0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0x05, 0x01, 0x09, 0x30,
            0x09, 0x31, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x02, 0x81, 0x06, 0xC0, 0xC0,
        ]);
        assert_eq!(compilation.item_count, 26);
    }

    #[test]
    fn accepts_every_standard_item_family() {
        let source = r"
            Usage Page (Generic Desktop, size=1)
            Usage (Mouse, size=1)
            Collection (Application)
              Push
              Usage Page (Generic Desktop)
              Logical Minimum (-100, size=2)
              Logical Maximum (100, size=2)
              Physical Minimum (-10)
              Physical Maximum (10)
              Unit Exponent (-2)
              Unit (SI Linear, length=1, time=-1, size=4)
              Report Size (8)
              Report ID (1, size=1)
              Report Count (1)
              Usage (X)
              Usage Minimum (X)
              Usage Maximum (Y)
              Designator Index (1)
              Designator Minimum (1)
              Designator Maximum (2)
              String Index (1)
              String Minimum (1)
              String Maximum (2)
              Delimiter (Open)
              Usage (Y)
              Delimiter (Close)
              Input (Data, Variable, Absolute, size=2)
              Usage (X)
              Output (Data, Variable, Absolute)
              Usage (Y)
              Feature (Data, Variable, Absolute)
              Pop
            End Collection
        ";
        let compilation = compile(source).unwrap();
        assert_ne!(compilation.bytes, [] as [u8; 0]);
        assert!(compilation.item_count >= 30);
    }

    #[test]
    fn raw_escape_hatches_cover_future_items() {
        let compilation = compile(
            r"
                Raw Short (type=Reserved, tag=15, data=[1, 2, 3, 4])
                Long Item (tag=0xaa, data=[0x10, 0x20, 0x30])
                Raw Bytes ([0x00])
            ",
        )
        .unwrap();
        assert_eq!(compilation.bytes, [
            0xFF, 1, 2, 3, 4, 0xFE, 3, 0xAA, 0x10, 0x20, 0x30, 0x00
        ]);
        assert!(
            compilation
                .warnings
                .iter()
                .any(|diagnostic| diagnostic.level == DiagnosticLevel::Warning)
        );
    }

    #[test]
    fn numeric_unit_can_use_an_exact_width() {
        let compilation = compile(
            r"
                Usage Page (Generic Desktop)
                Usage (Mouse)
                Collection (Application)
                  Unit (0x0011, size=2)
                End Collection
            ",
        )
        .unwrap();
        assert!(compilation.bytes.windows(3).any(|bytes| bytes == [0x66, 0x11, 0x00]));
    }

    #[test]
    fn rejects_invalid_widths_duplicate_keys_and_open_delimiters() {
        let diagnostics = compile(
            r"
                Usage Page (Generic Desktop, size=4)
                Usage (Mouse, size=1, SIZE=2)
                Collection (Application)
                  Delimiter (Open)
                  End Collection
            ",
        )
        .unwrap_err();
        let text = diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Usage Page cannot use size=4"));
        assert!(text.contains("Duplicate named argument"));
        assert!(text.contains("delimiter close"));
    }

    #[test]
    fn rejects_unpaired_reversed_and_dangling_local_items() {
        let diagnostics = compile(
            r"
                Usage Page (Generic Desktop)
                Usage Minimum (X)
                Designator Minimum (4)
                Designator Maximum (2)
                String Maximum (3)
                Report Size (8)
                Report Count (1)
                Input (Data, Array, Absolute)
                Usage (Y)
            ",
        )
        .unwrap_err();
        let text = diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Usage minimum has no usage maximum"));
        assert!(text.contains("Designator minimum exceeds designator maximum"));
        assert!(text.contains("String maximum has no string minimum"));
        assert!(text.contains("Local item is not followed by a main item"));
    }

    #[test]
    fn raw_future_main_consumes_local_state() {
        let diagnostics = compile(
            r"
                Usage Page (Generic Desktop)
                Usage (Mouse)
                Raw Short (type=Main, tag=0, data=[])
                Collection (Application)
                End Collection
            ",
        )
        .unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("requires a preceding usage"))
        );
    }

    #[test]
    fn main_flags_accept_raw_bits_and_size_together() {
        let compilation = compile(
            r"
                Usage Page (Generic Desktop)
                Usage (Mouse)
                Collection (Application)
                  Report Size (8)
                  Report Count (1)
                  Input (bits=0x102, size=2)
                End Collection
            ",
        )
        .unwrap();
        assert!(compilation.bytes.windows(3).any(|bytes| bytes == [0x82, 0x02, 0x01]));
    }
}
