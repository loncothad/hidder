//! Internal workspace automation for catalog generation and specification
//! updates.

mod generate;
mod json;
mod metadata;
mod sha256;

use std::{
    env,
    fs,
    path::{
        Path,
        PathBuf,
    },
    process::{
        self,
        Command,
    },
};

const GENERATED: &str = "crates/hid-usage-tables/src/generated.rs";
const SOURCE_JSON: &str = "spec/upstream/HidUsageTables.json";

fn main() {
    if let Err(error) = run() {
        eprintln!("Xtask: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "Xtask has no workspace parent".to_string())?;
    let mut arguments = env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "help".to_string());
    let rest: Vec<String> = arguments.collect();
    match command.as_str() {
        | "generate" => {
            let check = parse_only_flag(&rest, "--check")?;
            generate_command(workspace, check)
        },
        | "check" => {
            if !rest.is_empty() {
                return Err("`check` Takes no arguments".to_string());
            }
            generate_command(workspace, true)
        },
        | "verify" => {
            if !rest.is_empty() {
                return Err("`verify` Takes no arguments".to_string());
            }
            verify(workspace)
        },
        | "stats" => {
            if !rest.is_empty() {
                return Err("`stats` Takes no arguments".to_string());
            }
            let (_, stats) = generate::generate(workspace)?;
            print_stats(stats);
            Ok(())
        },
        | "extract" => extract_command(workspace, &rest),
        | "update" => update_command(workspace, &rest),
        | "help" | "--help" | "-h" => {
            help();
            Ok(())
        },
        | other => Err(format!("Unknown command `{other}`; run `cargo xtask help`")),
    }
}

fn generate_command(workspace: &Path, check: bool) -> Result<(), String> {
    let (generated, stats) = generate::generate(workspace)?;
    let path = workspace.join(GENERATED);
    if check {
        let current = fs::read_to_string(&path).map_err(|error| format!("Read {}: {error}", path.display()))?;
        if current != generated {
            return Err(format!("{} is stale; run `cargo xtask generate`", path.display()));
        }
        println!("Generated catalog is current");
    } else {
        fs::write(&path, generated).map_err(|error| format!("Write {}: {error}", path.display()))?;
        println!("Wrote {}", path.display());
    }
    print_stats(stats);
    Ok(())
}

fn verify(workspace: &Path) -> Result<(), String> {
    let source_metadata = metadata::read(workspace)?;
    let source_path = workspace.join(SOURCE_JSON);
    let source = fs::read(&source_path).map_err(|error| format!("Read {}: {error}", source_path.display()))?;
    let actual_hash = sha256::digest_hex(&source);
    if actual_hash != source_metadata.hut_json_sha256.to_ascii_lowercase() {
        return Err(format!(
            "SHA-256 mismatch for {}: expected {}, got {}",
            source_path.display(),
            source_metadata.hut_json_sha256,
            actual_hash
        ));
    }
    let parsed = json::parse(
        std::str::from_utf8(&source).map_err(|error| format!("{} is not UTF-8: {error}", source_path.display()))?,
    )?;
    let actual_version = json_version(&parsed)?;
    if actual_version != source_metadata.hut_version {
        return Err(format!(
            "Source JSON is HUT {actual_version}, manifest expects {}",
            source_metadata.hut_version
        ));
    }
    generate_command(workspace, true)?;
    println!(
        "Verified HUT {} source SHA-256 {}",
        source_metadata.hut_version, actual_hash
    );
    Ok(())
}

fn extract_command(workspace: &Path, arguments: &[String]) -> Result<(), String> {
    if arguments.is_empty() || arguments.len() > 2 {
        return Err("Usage: cargo xtask extract <HUT.PDF> [output.JSON]".to_string());
    }
    let pdf = PathBuf::from(&arguments[0]);
    let output = arguments
        .get(1)
        .map_or_else(|| workspace.join(SOURCE_JSON), PathBuf::from);
    let attachment = metadata::read(workspace)?.hut_json_attachment;
    extract_attachment(&pdf, &output, &attachment)?;
    let bytes = fs::read(&output).map_err(|error| format!("Read {}: {error}", output.display()))?;
    println!(
        "Extracted {} ({} bytes, SHA-256 {})",
        output.display(),
        bytes.len(),
        sha256::digest_hex(&bytes)
    );
    Ok(())
}

fn update_command(workspace: &Path, arguments: &[String]) -> Result<(), String> {
    if arguments.is_empty() {
        return Err("Usage: cargo xtask update <HUT.PDF> --publication-date yyyy-mm-dd [--url url]".to_string());
    }
    let pdf = PathBuf::from(&arguments[0]);
    let mut publication_date = None;
    let mut url = None;
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            | "--publication-date" => {
                index += 1;
                publication_date = arguments.get(index).cloned();
            },
            | "--url" => {
                index += 1;
                url = arguments.get(index).cloned();
            },
            | other => return Err(format!("Unknown update argument `{other}`")),
        }
        index += 1;
    }
    let publication_date = publication_date
        .ok_or_else(|| "`update` requires `--publication-date YYYY-MM-DD` from the USB-IF document page".to_string())?;
    validate_date(&publication_date)?;

    let current_metadata = metadata::read(workspace)?;
    let source_path = workspace.join(SOURCE_JSON);
    let manifest_path = workspace.join("spec/source.toml");
    let generated_path = workspace.join(GENERATED);
    let next_path = source_path.with_extension("json.next");
    extract_attachment(&pdf, &next_path, &current_metadata.hut_json_attachment)?;
    let next = fs::read(&next_path).map_err(|error| format!("Read {}: {error}", next_path.display()))?;
    let parsed = json::parse(
        std::str::from_utf8(&next).map_err(|error| format!("{} is not UTF-8: {error}", next_path.display()))?,
    )?;
    let version = json_version(&parsed)?;
    parsed.get("UsagePages")?.as_array()?;
    let digest = sha256::digest_hex(&next);

    let old_source = fs::read(&source_path).map_err(|error| format!("Read {}: {error}", source_path.display()))?;
    let old_manifest =
        fs::read(&manifest_path).map_err(|error| format!("Read {}: {error}", manifest_path.display()))?;
    let old_generated =
        fs::read(&generated_path).map_err(|error| format!("Read {}: {error}", generated_path.display()))?;

    let result = (|| {
        fs::write(&source_path, &next).map_err(|error| format!("Write {}: {error}", source_path.display()))?;
        metadata::update_hut(workspace, &version, &publication_date, &digest, url.as_deref())?;
        let (generated, stats) = generate::generate(workspace)?;
        fs::write(&generated_path, generated)
            .map_err(|error| format!("Write {}: {error}", generated_path.display()))?;
        print_stats(stats);
        Ok::<(), String>(())
    })();
    let _ = fs::remove_file(&next_path);
    if let Err(error) = result {
        let _ = fs::write(&source_path, old_source);
        let _ = fs::write(&manifest_path, old_manifest);
        let _ = fs::write(&generated_path, old_generated);
        return Err(format!("Update failed and was rolled back: {error}"));
    }
    println!("Updated to HUT {version} ({publication_date}), SHA-256 {digest}; review supplements and commit the diff");
    Ok(())
}

fn extract_attachment(pdf: &Path, output: &Path, attachment: &str) -> Result<(), String> {
    if !pdf.is_file() {
        return Err(format!("PDF does not exist: {}", pdf.display()));
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("Create {}: {error}", parent.display()))?;
    }
    let status = Command::new("pdfdetach")
        .arg("-savefile")
        .arg(attachment)
        .arg("-o")
        .arg(output)
        .arg(pdf)
        .status()
        .map_err(|error| {
            format!("Run `pdfdetach`: {error}; install poppler utilities to extract the PDF attachment")
        })?;
    if !status.success() {
        return Err(format!(
            "Pdfdetach failed with status {status} while extracting `{attachment}`"
        ));
    }
    Ok(())
}

fn json_version(root: &json::Json) -> Result<String, String> {
    let major = root.get("UsageTableVersion")?.as_i64()?;
    let revision = root.get("UsageTableRevision")?.as_i64()?;
    let sub = root.get("UsageTableSubRevisionInternal")?.as_i64()?;
    Ok(format!("{major}.{revision}.{sub}"))
}

fn validate_date(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(format!("Invalid publication date `{value}`; expected yyyy-mm-dd"));
    }
    Ok(())
}

fn parse_only_flag(arguments: &[String], allowed: &str) -> Result<bool, String> {
    match arguments {
        | [] => Ok(false),
        | [value] if value == allowed => Ok(true),
        | _ => Err(format!("Only `{allowed}` is accepted here")),
    }
}

fn print_stats(stats: generate::Stats) {
    println!(
        "Catalog: {} pages, {} named usages, {} aliases, {} usage ranges, {} page ranges",
        stats.pages, stats.usages, stats.aliases, stats.usage_ranges, stats.page_ranges
    );
}

fn help() {
    println!(
        "\
HID report toolkit code generator

USAGE:
    cargo xtask generate [--check]
    cargo xtask check
    cargo xtask verify
    cargo xtask stats
    cargo xtask extract <hut.pdf> [output.json]
    cargo xtask update <hut.pdf> --publication-date YYYY-MM-DD [--url URL]

`update` extracts the embedded JSON with Poppler's pdfdetach, updates the source
manifest transactionally, and regenerates the Rust catalog. Review aliases,
deprecations, external pages, and page ranges after every specification update."
    );
}
