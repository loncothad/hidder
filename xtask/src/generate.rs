use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    fmt::Write as _,
    fs,
    path::Path,
};

use crate::{
    json::{
        self,
        Json,
    },
    metadata,
};

#[derive(Clone, Debug)]
struct SourceUsage {
    id:    u16,
    name:  String,
    kinds: Vec<String>,
}

#[derive(Clone, Debug)]
struct Generator {
    prefix: String,
    start:  u16,
    end:    u16,
    kinds:  Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PageClass {
    Undefined,
    Defined,
    Generated,
    External,
}

impl PageClass {
    fn rust(self) -> &'static str {
        match self {
            | Self::Undefined => "Undefined",
            | Self::Defined => "Defined",
            | Self::Generated => "Generated",
            | Self::External => "External",
        }
    }
}

#[derive(Clone, Debug)]
struct Page {
    id:          u16,
    name:        String,
    class:       PageClass,
    usages:      Vec<SourceUsage>,
    generator:   Option<Generator>,
    usage_start: usize,
    usage_len:   usize,
    range_start: usize,
    range_len:   usize,
}

#[derive(Clone, Debug)]
struct Alias {
    page: u16,
    id:   u16,
    name: String,
    kind: String,
}

#[derive(Clone, Debug)]
struct FlatUsage {
    page:        u16,
    id:          u16,
    name:        String,
    slug:        String,
    bits:        u32,
    status:      &'static str,
    alias_start: usize,
    alias_len:   usize,
}

#[derive(Clone, Copy, Debug)]
struct UsageRange {
    page:  u16,
    start: u16,
    end:   u16,
}

#[derive(Clone, Debug)]
struct PageRange {
    start: u16,
    end:   u16,
    kind:  &'static str,
    label: String,
}

#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub pages:        usize,
    pub usages:       usize,
    pub aliases:      usize,
    pub usage_ranges: usize,
    pub page_ranges:  usize,
}

pub fn generate(workspace: &Path) -> Result<(String, Stats), String> {
    let metadata = metadata::read(workspace)?;
    let source_path = workspace.join("spec/upstream/HidUsageTables.json");
    let source_text =
        fs::read_to_string(&source_path).map_err(|error| format!("Read {}: {error}", source_path.display()))?;
    let root = json::parse(&source_text)?;
    verify_version(&root, &metadata.hut_version)?;
    let last_generated = root.get("LastGenerated")?.as_str()?;

    let mut pages = source_pages(&root)?;
    pages.extend(extra_pages(&workspace.join("spec/pages.csv"))?);
    pages.sort_by_key(|page| page.id);
    for pair in pages.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(format!("Duplicate page {:#06x}", pair[0].id));
        }
    }

    let alias_source = aliases(&workspace.join("spec/aliases.csv"))?;
    let status_source = statuses(&workspace.join("spec/status.csv"))?;
    let mut page_ranges = page_ranges(&workspace.join("spec/page-ranges.csv"))?;
    validate_page_partition(&pages, &mut page_ranges)?;

    let mut aliases_by_usage: BTreeMap<(u16, u16), Vec<Alias>> = BTreeMap::new();
    for alias in alias_source {
        aliases_by_usage.entry((alias.page, alias.id)).or_default().push(alias);
    }

    let mut flat_aliases = Vec::new();
    let mut flat_usages = Vec::new();
    let mut usage_ranges = Vec::new();
    let mut known_usage_keys = BTreeSet::new();

    for page in &mut pages {
        page.usages.sort_by_key(|usage| usage.id);
        for pair in page.usages.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(format!("Duplicate usage {:#06x}:{:#06x}", page.id, pair[0].id));
            }
        }
        page.usage_start = flat_usages.len();
        for usage in &page.usages {
            let key = (page.id, usage.id);
            known_usage_keys.insert(key);
            let entry_aliases = aliases_by_usage.get(&key).map_or(&[][..], Vec::as_slice);
            let alias_start = flat_aliases.len();
            flat_aliases.extend_from_slice(entry_aliases);
            flat_usages.push(FlatUsage {
                page: page.id,
                id: usage.id,
                name: usage.name.clone(),
                slug: slug(&usage.name, &format!("Usage_{:04x}", usage.id)),
                bits: type_bits(&usage.kinds)?,
                status: if status_source.get(&key).map(String::as_str) == Some("deprecated") {
                    "Deprecated"
                } else {
                    "Current"
                },
                alias_start,
                alias_len: entry_aliases.len(),
            });
        }
        page.usage_len = flat_usages.len() - page.usage_start;
        page.range_start = usage_ranges.len();
        match page.class {
            | PageClass::Defined => {
                let mut cursor = 0_u32;
                for usage in &page.usages {
                    let id = u32::from(usage.id);
                    if cursor < id {
                        usage_ranges.push(UsageRange {
                            page:  page.id,
                            start: cursor as u16,
                            end:   (id - 1) as u16,
                        });
                    }
                    cursor = id + 1;
                }
                if cursor <= u32::from(u16::MAX) {
                    usage_ranges.push(UsageRange {
                        page:  page.id,
                        start: cursor as u16,
                        end:   u16::MAX,
                    });
                }
            },
            | PageClass::Generated => {
                let generator = page
                    .generator
                    .as_ref()
                    .ok_or_else(|| format!("Generated page {} lacks generator", page.name))?;
                if generator.start > 0 {
                    usage_ranges.push(UsageRange {
                        page:  page.id,
                        start: 0,
                        end:   generator.start - 1,
                    });
                }
                if generator.end < u16::MAX {
                    usage_ranges.push(UsageRange {
                        page:  page.id,
                        start: generator.end + 1,
                        end:   u16::MAX,
                    });
                }
            },
            | PageClass::Undefined | PageClass::External => {},
        }
        page.range_len = usage_ranges.len() - page.range_start;
    }

    for key in aliases_by_usage.keys() {
        if !known_usage_keys.contains(key) {
            return Err(format!("Alias references missing usage {:#06x}:{:#06x}", key.0, key.1));
        }
    }
    for key in status_source.keys() {
        if !known_usage_keys.contains(key) {
            return Err(format!("Status references missing usage {:#06x}:{:#06x}", key.0, key.1));
        }
    }

    let mut output = String::new();
    output.push_str("// @generated by `cargo xtask generate`; do not edit by hand.\n");
    output.push_str(
        "use crate::{AliasEntry, AliasKind, IdRange, PageClass, PageRangeEntry, PageRangeKind, UsageEntry, \
         UsageGenerator, UsagePageEntry, UsageRangeEntry, UsageRangeKind, UsageStatus, UsageTypeSet};\n\n",
    );
    writeln!(output, "/// HID Usage Tables source revision.").unwrap();
    writeln!(
        output,
        "pub const HUT_VERSION: &str = {};",
        rust_string(&metadata.hut_version)
    )
    .unwrap();
    writeln!(output, "/// Publication date of the source revision.").unwrap();
    writeln!(
        output,
        "pub const HUT_PUBLICATION_DATE: &str = {};",
        rust_string(&metadata.hut_publication_date)
    )
    .unwrap();
    writeln!(output, "/// Timestamp recorded by the machine-readable HUT attachment.").unwrap();
    writeln!(
        output,
        "pub const HUT_LAST_GENERATED: &str = {};",
        rust_string(last_generated)
    )
    .unwrap();
    writeln!(output, "/// Name of the PDF's embedded machine-readable attachment.").unwrap();
    writeln!(
        output,
        "pub const HUT_JSON_ATTACHMENT: &str = {};",
        rust_string(&metadata.hut_json_attachment)
    )
    .unwrap();
    writeln!(output, "/// SHA-256 of the checked-in machine-readable HUT attachment.").unwrap();
    writeln!(
        output,
        "pub const HUT_JSON_SHA256: &str = {};",
        rust_string(&metadata.hut_json_sha256)
    )
    .unwrap();
    writeln!(output, "/// Canonical source URL for this HUT revision.").unwrap();
    writeln!(
        output,
        "pub const HUT_SOURCE_URL: &str = {};",
        rust_string(&metadata.hut_url)
    )
    .unwrap();
    writeln!(
        output,
        "/// HID Device Class Definition revision used by the descriptor model."
    )
    .unwrap();
    writeln!(
        output,
        "pub const HID_SPEC_VERSION: &str = {};",
        rust_string(&metadata.hid_version)
    )
    .unwrap();
    writeln!(output, "/// Canonical source URL for the HID Device Class Definition.").unwrap();
    writeln!(
        output,
        "pub const HID_SPEC_URL: &str = {};\n",
        rust_string(&metadata.hid_url)
    )
    .unwrap();
    writeln!(output, "/// Number of individually named source usages.").unwrap();
    writeln!(output, "pub const NAMED_USAGE_COUNT: usize = {};", flat_usages.len()).unwrap();
    writeln!(output, "/// Number of named or externally identified usage pages.").unwrap();
    writeln!(output, "pub const PAGE_COUNT: usize = {};\n", pages.len()).unwrap();

    output.push_str("/// Alternate usage names retained from source spellings and supplements.\n");
    output.push_str("pub static ALIASES: &[AliasEntry] = &[\n");
    for alias in &flat_aliases {
        writeln!(
            output,
            "    AliasEntry {{ page: 0x{:04x}, id: 0x{:04x}, name: {}, kind: AliasKind::{} }},",
            alias.page,
            alias.id,
            rust_string(&alias.name),
            alias.kind
        )
        .unwrap();
    }
    output.push_str("];\n\n");

    output.push_str("/// Individually named usage records, sorted by `(page, id)`.\n");
    output.push_str("pub static USAGES: &[UsageEntry] = &[\n");
    for usage in &flat_usages {
        writeln!(
            output,
            "    UsageEntry {{ page: 0x{:04x}, id: 0x{:04x}, name: {}, slug: {}, types: \
             UsageTypeSet::from_bits(0x{:08x}), status: UsageStatus::{}, alias_start: {}, alias_len: {} }},",
            usage.page,
            usage.id,
            rust_string(&usage.name),
            rust_string(&usage.slug),
            usage.bits,
            usage.status,
            usage.alias_start,
            usage.alias_len
        )
        .unwrap();
    }
    output.push_str("];\n\n");

    output.push_str("/// Reserved/unassigned usage-ID ranges, grouped by page.\n");
    output.push_str("pub static USAGE_RANGES: &[UsageRangeEntry] = &[\n");
    for range in &usage_ranges {
        writeln!(
            output,
            "    UsageRangeEntry {{ page: 0x{:04x}, range: IdRange::new(0x{:04x}, 0x{:04x}), kind: \
             UsageRangeKind::Reserved }},",
            range.page, range.start, range.end
        )
        .unwrap();
    }
    output.push_str("];\n\n");

    output.push_str("/// Named pages, sorted by numeric identifier.\n");
    output.push_str("pub static PAGES: &[UsagePageEntry] = &[\n");
    for page in &pages {
        let generator = if let Some(generator) = &page.generator {
            format!(
                "Some(UsageGenerator {{ name_prefix: {}, range: IdRange::new(0x{:04x}, 0x{:04x}), types: \
                 UsageTypeSet::from_bits(0x{:08x}) }})",
                rust_string(&generator.prefix),
                generator.start,
                generator.end,
                type_bits(&generator.kinds)?
            )
        } else {
            "None".to_string()
        };
        writeln!(
            output,
            "    UsagePageEntry {{ id: 0x{:04x}, name: {}, slug: {}, class: PageClass::{}, usage_start: {}, \
             usage_len: {}, generator: {}, range_start: {}, range_len: {} }},",
            page.id,
            rust_string(&page.name),
            rust_string(&slug(&page.name, &format!("Page_{:04x}", page.id))),
            page.class.rust(),
            page.usage_start,
            page.usage_len,
            generator,
            page.range_start,
            page.range_len
        )
        .unwrap();
    }
    output.push_str("];\n\n");

    writeln!(
        output,
        "/// Reserved and vendor-defined ranges from HUT {} Table 3.1.",
        metadata.hut_version
    )
    .unwrap();
    output.push_str("pub static PAGE_RANGES: &[PageRangeEntry] = &[\n");
    for range in &page_ranges {
        writeln!(
            output,
            "    PageRangeEntry {{ range: IdRange::new(0x{:04x}, 0x{:04x}), kind: PageRangeKind::{}, label: {} }},",
            range.start,
            range.end,
            range.kind,
            rust_string(&range.label)
        )
        .unwrap();
    }
    output.push_str("];\n");

    let stats = Stats {
        pages:        pages.len(),
        usages:       flat_usages.len(),
        aliases:      flat_aliases.len(),
        usage_ranges: usage_ranges.len(),
        page_ranges:  page_ranges.len(),
    };
    Ok((output, stats))
}

fn verify_version(root: &Json, expected: &str) -> Result<(), String> {
    let major = root.get("UsageTableVersion")?.as_i64()?;
    let revision = root.get("UsageTableRevision")?.as_i64()?;
    let sub = root.get("UsageTableSubRevisionInternal")?.as_i64()?;
    let actual = format!("{major}.{revision}.{sub}");
    if actual != expected {
        return Err(format!(
            "source JSON is HUT {actual}, but spec/source.toml expects {expected}; update and review the source \
             metadata before generating"
        ));
    }
    Ok(())
}

fn source_pages(root: &Json) -> Result<Vec<Page>, String> {
    let mut pages = Vec::new();
    for value in root.get("UsagePages")?.as_array()? {
        let kind = value.get("Kind")?.as_str()?;
        let class = match kind {
            | "Defined" => PageClass::Defined,
            | "Generated" => PageClass::Generated,
            | other => return Err(format!("Unknown page kind `{other}`")),
        };
        let id = to_u16(value.get("Id")?.as_i64()?, "page ID")?;
        let name = value.get("Name")?.as_str()?.to_string();
        let mut usages = Vec::new();
        for usage in value.get("UsageIds")?.as_array()? {
            let usage_id = to_u16(usage.get("Id")?.as_i64()?, "usage ID")?;
            let usage_name = usage.get("Name")?.as_str()?.to_string();
            let kinds = string_array(usage.get("Kinds")?)?;
            usages.push(SourceUsage {
                id: usage_id,
                name: usage_name,
                kinds,
            });
        }
        let generator_value = value.get("UsageIdGenerator")?;
        let generator = if generator_value.is_null() {
            None
        } else {
            Some(Generator {
                prefix: generator_value.get("NamePrefix")?.as_str()?.to_string(),
                start:  to_u16(generator_value.get("StartUsageId")?.as_i64()?, "generator start")?,
                end:    to_u16(generator_value.get("EndUsageId")?.as_i64()?, "generator end")?,
                kinds:  string_array(generator_value.get("Kinds")?)?,
            })
        };
        match (class, generator.as_ref()) {
            | (PageClass::Defined, None) | (PageClass::Generated, Some(_)) => {},
            | (PageClass::Defined, Some(_)) => {
                return Err(format!("Defined page {id:#06x} unexpectedly has a generator"));
            },
            | (PageClass::Generated, None) => {
                return Err(format!("Generated page {id:#06x} lacks a generator"));
            },
            | (PageClass::Undefined | PageClass::External, _) => unreachable!(),
        }
        if let Some(generator) = &generator {
            if generator.start > generator.end {
                return Err(format!("Page {id:#06x} has a reversed generator range"));
            }
        }
        pages.push(Page {
            id,
            name,
            class,
            usages,
            generator,
            usage_start: 0,
            usage_len: 0,
            range_start: 0,
            range_len: 0,
        });
    }
    Ok(pages)
}

fn extra_pages(path: &Path) -> Result<Vec<Page>, String> {
    let mut pages = Vec::new();
    for row in csv_rows(path)? {
        if row.len() < 3 {
            return Err(format!("{}: Expected at least 3 columns", path.display()));
        }
        let class = match row[1].as_str() {
            | "undefined" => PageClass::Undefined,
            | "external" => PageClass::External,
            | other => return Err(format!("{}: Unknown page class `{other}`", path.display())),
        };
        pages.push(Page {
            id: parse_u16(&row[0])?,
            name: row[2].clone(),
            class,
            usages: Vec::new(),
            generator: None,
            usage_start: 0,
            usage_len: 0,
            range_start: 0,
            range_len: 0,
        });
    }
    Ok(pages)
}

fn aliases(path: &Path) -> Result<Vec<Alias>, String> {
    let mut result = Vec::new();
    for row in csv_rows(path)? {
        if row.len() < 4 {
            return Err(format!("{}: Expected at least 4 columns", path.display()));
        }
        let kind = match row[3].as_str() {
            | "specification" => "Specification",
            | "legacy" => "Legacy",
            | "search" => "Search",
            | other => return Err(format!("{}: Unknown alias kind `{other}`", path.display())),
        };
        result.push(Alias {
            page: parse_u16(&row[0])?,
            id:   parse_u16(&row[1])?,
            name: row[2].clone(),
            kind: kind.to_string(),
        });
    }
    Ok(result)
}

fn statuses(path: &Path) -> Result<BTreeMap<(u16, u16), String>, String> {
    let mut result = BTreeMap::new();
    for row in csv_rows(path)? {
        if row.len() < 3 {
            return Err(format!("{}: Expected at least 3 columns", path.display()));
        }
        if row[2] != "deprecated" && row[2] != "current" {
            return Err(format!("{}: Unknown status `{}`", path.display(), row[2]));
        }
        let key = (parse_u16(&row[0])?, parse_u16(&row[1])?);
        if result.insert(key, row[2].clone()).is_some() {
            return Err(format!(
                "{}: Duplicate status {:#06x}:{:#06x}",
                path.display(),
                key.0,
                key.1
            ));
        }
    }
    Ok(result)
}

fn validate_page_partition(pages: &[Page], ranges: &mut [PageRange]) -> Result<(), String> {
    ranges.sort_by_key(|range| (range.start, range.end));

    let mut segments = Vec::with_capacity(pages.len() + ranges.len());
    for page in pages {
        segments.push((
            u32::from(page.id),
            u32::from(page.id),
            format!("Page {:#06x} ({})", page.id, page.name),
        ));
    }
    for range in ranges.iter() {
        segments.push((
            u32::from(range.start),
            u32::from(range.end),
            format!("{} range {:#06x}..={:#06x}", range.kind, range.start, range.end),
        ));
    }
    segments.sort_by_key(|segment| (segment.0, segment.1));

    let mut expected = 0_u32;
    for (start, end, label) in segments {
        if start < expected {
            return Err(format!(
                "Page table segment `{label}` overlaps the preceding page or range at {start:#06x}"
            ));
        }
        if start > expected {
            return Err(format!(
                "Page table leaves {expected:#06x}..={:#06x} unclassified",
                start - 1
            ));
        }
        expected = end + 1;
    }
    if expected != 0x1_0000 {
        return Err(format!("Page table leaves {expected:#06x}..=0xffff unclassified"));
    }
    Ok(())
}

fn page_ranges(path: &Path) -> Result<Vec<PageRange>, String> {
    let mut result = Vec::new();
    for row in csv_rows(path)? {
        if row.len() < 4 {
            return Err(format!("{}: Expected 4 columns", path.display()));
        }
        let kind = match row[2].as_str() {
            | "reserved" => "Reserved",
            | "vendor_defined" => "VendorDefined",
            | other => return Err(format!("{}: Unknown range kind `{other}`", path.display())),
        };
        let start = parse_u16(&row[0])?;
        let end = parse_u16(&row[1])?;
        if start > end {
            return Err(format!("{}: Reversed page range", path.display()));
        }
        result.push(PageRange {
            start,
            end,
            kind,
            label: row[3].clone(),
        });
    }
    Ok(result)
}

fn string_array(value: &Json) -> Result<Vec<String>, String> {
    value
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(ToString::to_string))
        .collect()
}

fn type_bits(kinds: &[String]) -> Result<u32, String> {
    let mut bits = 0_u32;
    for kind in kinds {
        let index = match kind.as_str() {
            | "LC" => 0,
            | "OOC" => 1,
            | "MC" => 2,
            | "OSC" => 3,
            | "RTC" => 4,
            | "Sel" => 5,
            | "SV" => 6,
            | "SF" => 7,
            | "DV" => 8,
            | "DF" => 9,
            | "NAry" => 10,
            | "CA" => 11,
            | "CL" => 12,
            | "CP" => 13,
            | "US" => 14,
            | "UM" => 15,
            | "BufferedBytes" => 16,
            | other => return Err(format!("Unknown usage type `{other}`")),
        };
        bits |= 1_u32 << index;
    }
    Ok(bits)
}

fn csv_rows(path: &Path) -> Result<Vec<Vec<String>>, String> {
    let source = fs::read_to_string(path).map_err(|error| format!("Read {}: {error}", path.display()))?;
    let mut result = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        result.push(parse_csv_line(line).map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?);
    }
    Ok(result)
}

fn parse_csv_line(line: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(character);
            }
        } else {
            match character {
                | '"' if field.is_empty() => quoted = true,
                | ',' => {
                    result.push(field.trim().to_string());
                    field.clear();
                },
                | _ => field.push(character),
            }
        }
    }
    if quoted {
        return Err("Unterminated quoted csv field".to_string());
    }
    result.push(field.trim().to_string());
    Ok(result)
}

fn parse_u16(source: &str) -> Result<u16, String> {
    let source = source.trim();
    let (radix, digits) = if let Some(value) = source.strip_prefix("0x") {
        (16, value)
    } else {
        (10, source)
    };
    u16::from_str_radix(digits, radix).map_err(|_| format!("Invalid u16 `{source}`"))
}

fn to_u16(value: i64, context: &str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{context} {value} Is outside u16"))
}

fn slug(name: &str, fallback: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for mut character in name.chars() {
        character = fold_latin(character);
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else if !output.is_empty() && !separator {
            output.push('_');
            separator = true;
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        output.push_str(fallback);
    }
    if output.as_bytes()[0].is_ascii_digit() {
        output.insert_str(0, "u_");
    }
    output
}

fn fold_latin(character: char) -> char {
    match character {
        | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        | 'Ç' | 'ç' => 'c',
        | 'È' | 'É' | 'Ê' | 'Ë' | 'è' | 'é' | 'ê' | 'ë' => 'e',
        | 'Ì' | 'Í' | 'Î' | 'Ï' | 'ì' | 'í' | 'î' | 'ï' => 'i',
        | 'Ñ' | 'ñ' => 'n',
        | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        | 'Ù' | 'Ú' | 'Û' | 'Ü' | 'ù' | 'ú' | 'û' | 'ü' => 'u',
        | 'Ý' | 'ý' | 'ÿ' => 'y',
        | other => other,
    }
}

fn rust_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            | '\\' => output.push_str("\\\\"),
            | '"' => output.push_str("\\\""),
            | '\n' => output.push_str("\\n"),
            | '\r' => output.push_str("\\r"),
            | '\t' => output.push_str("\\t"),
            | '\u{0008}' => output.push_str("\\b"),
            | '\u{000c}' => output.push_str("\\f"),
            | value if value.is_control() => write!(output, "\\u{{{:x}}}", value as u32).unwrap(),
            | value => output.push(value),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_match_generated_policy() {
        assert_eq!(slug("Horizontal Moiré", "fallback"), "horizontal_moire");
        assert_eq!(slug("1 Axis", "fallback"), "u_1_axis");
    }

    #[test]
    fn rust_strings_escape_backslashes() {
        assert_eq!(rust_string(r"a\b"), r#""a\\b""#);
        assert_eq!(rust_string("\u{1}"), r#""\u{1}""#);
    }
}
