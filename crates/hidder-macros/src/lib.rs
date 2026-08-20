#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Procedural macros for compiling textual HID descriptors into byte arrays.

extern crate proc_macro;

use std::{
    fmt::Write as _,
    str::FromStr,
};

use proc_macro::{
    TokenStream,
    TokenTree,
};

/// Compiles one string literal into an inferred-length `[u8; N]` expression.
///
/// The DSL deliberately resembles the notation used by HID descriptor tools:
///
/// ```
/// use hidder_macros::hid_report;
///
/// static MOUSE: &[u8] = &hid_report!(
///     r#"
///     Usage Page (Generic Desktop)
///     Usage (Mouse)
///     Collection (Application)
///       Usage (Pointer)
///       Collection (Physical)
///         Usage Page (Generic Desktop)
///         Usage (X)
///         Usage (Y)
///         Logical Minimum (-127)
///         Logical Maximum (127)
///         Report Size (8)
///         Report Count (2)
///         Input (Data, Variable, Relative)
///       End Collection
///     End Collection
/// "#
/// );
/// # assert!(!MOUSE.is_empty());
/// ```
#[proc_macro]
pub fn hid_report(input: TokenStream) -> TokenStream {
    let source = match single_string_literal(input) {
        | Ok(source) => source,
        | Err(message) => return compile_errors(&[message]),
    };
    match hidder_dsl::compile(&source) {
        | Ok(compilation) => byte_array(&compilation.bytes),
        | Err(diagnostics) => {
            let messages: Vec<String> = diagnostics.into_iter().map(|value| value.to_string()).collect();
            compile_errors(&messages)
        },
    }
}

/// Compiles one descriptor string to its encoded byte length.
///
/// This is useful for APIs that require a separately declared array size.
#[proc_macro]
pub fn hid_report_len(input: TokenStream) -> TokenStream {
    let source = match single_string_literal(input) {
        | Ok(source) => source,
        | Err(message) => return compile_errors(&[message]),
    };
    match hidder_dsl::compile(&source) {
        | Ok(compilation) => parse_tokens(&compilation.bytes.len().to_string()),
        | Err(diagnostics) => {
            let messages: Vec<String> = diagnostics.into_iter().map(|value| value.to_string()).collect();
            compile_errors(&messages)
        },
    }
}

/// Declares a `const` or `static` with its `[u8; N]` type generated
/// automatically.
///
/// ```
/// use hidder_macros::hid_report_descriptor;
///
/// hid_report_descriptor! {
///     pub const VENDOR_PING = r#"
///         Usage Page (0xff00)
///         Usage (1)
///         Collection (Application)
///           Report Size (8)
///           Report Count (1)
///           Logical Minimum (0)
///           Logical Maximum (255)
///           Usage (2)
///           Input (Data, Variable, Absolute)
///         End Collection
///     "#;
/// }
/// # assert!(VENDOR_PING.len() > 4);
/// ```
#[proc_macro]
pub fn hid_report_descriptor(input: TokenStream) -> TokenStream {
    let declaration = match Declaration::parse(input) {
        | Ok(value) => value,
        | Err(message) => return compile_errors(&[message]),
    };
    match hidder_dsl::compile(&declaration.source) {
        | Ok(compilation) => {
            let array = byte_array_text(&compilation.bytes);
            let output = format!(
                "{} {} {}: [u8; {}] = {};",
                declaration.visibility,
                declaration.kind,
                declaration.name,
                compilation.bytes.len(),
                array
            );
            parse_tokens(&output)
        },
        | Err(diagnostics) => {
            let messages: Vec<String> = diagnostics.into_iter().map(|value| value.to_string()).collect();
            compile_errors(&messages)
        },
    }
}

struct Declaration {
    visibility: String,
    kind:       String,
    name:       String,
    source:     String,
}

impl Declaration {
    fn parse(input: TokenStream) -> Result<Self, String> {
        let tokens: Vec<TokenTree> = input.into_iter().collect();
        let kind_index = tokens
            .iter()
            .position(|token| matches!(token, TokenTree::Ident(value) if value.to_string() == "const" || value.to_string() == "static"))
            .ok_or_else(|| "Expected `const name = \"...\";` or `static name = \"...\";`".to_string())?;
        let visibility = tokens[.. kind_index]
            .iter()
            .cloned()
            .collect::<TokenStream>()
            .to_string();
        let Some(TokenTree::Ident(value)) = tokens.get(kind_index) else {
            return Err("Expected `const` or `static`".to_string());
        };
        let kind = value.to_string();
        let name = match tokens.get(kind_index + 1) {
            | Some(TokenTree::Ident(value)) => value.to_string(),
            | _ => return Err("Expected an identifier after `const`/`static`".to_string()),
        };
        match tokens.get(kind_index + 2) {
            | Some(TokenTree::Punct(value)) if value.as_char() == '=' => {},
            | _ => return Err("Expected `=` after descriptor name".to_string()),
        }
        let literal = match tokens.get(kind_index + 3) {
            | Some(TokenTree::Literal(value)) => value.clone(),
            | _ => return Err("Expected a string literal after `=`".to_string()),
        };
        for token in tokens.iter().skip(kind_index + 4) {
            match token {
                | TokenTree::Punct(value) if value.as_char() == ';' => {},
                | _ => return Err("Unexpected tokens after descriptor string literal".to_string()),
            }
        }
        Ok(Self {
            visibility,
            kind,
            name,
            source: decode_string_literal(&literal.to_string())?,
        })
    }
}

fn single_string_literal(input: TokenStream) -> Result<String, String> {
    let mut tokens = input.into_iter();
    let Some(TokenTree::Literal(value)) = tokens.next() else {
        return Err("Expected exactly one string literal".to_string());
    };
    let literal = value;
    if tokens.next().is_some() {
        return Err("Expected exactly one string literal".to_string());
    }
    decode_string_literal(&literal.to_string())
}

fn decode_string_literal(token: &str) -> Result<String, String> {
    if let Some(rest) = token.strip_prefix('r') {
        let hashes = rest.bytes().take_while(|byte| *byte == b'#').count();
        let opening = hashes;
        if rest.as_bytes().get(opening) != Some(&b'"') {
            return Err("Expected a raw string literal".to_string());
        }
        let suffix = format!("\"{}", "#".repeat(hashes));
        if !rest.ends_with(&suffix) {
            return Err("Malformed raw string literal".to_string());
        }
        let body_start = opening + 1;
        let body_end = rest.len() - suffix.len();
        return Ok(rest[body_start .. body_end].to_string());
    }

    if !token.starts_with('"') || !token.ends_with('"') || token.len() < 2 {
        return Err("Expected a UTF-8 string literal (byte strings are not supported)".to_string());
    }
    let body = &token[1 .. token.len() - 1];
    let mut output = String::new();
    let mut chars = body.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let escaped = chars
            .next()
            .ok_or_else(|| "Trailing backslash in string literal".to_string())?;
        match escaped {
            | '\\' => output.push('\\'),
            | '"' => output.push('"'),
            | 'n' => output.push('\n'),
            | 'r' => output.push('\r'),
            | 't' => output.push('\t'),
            | '0' => output.push('\0'),
            | 'x' => {
                let high = chars.next().ok_or_else(|| "Short `\\xnn` escape".to_string())?;
                let low = chars.next().ok_or_else(|| "Short `\\xnn` escape".to_string())?;
                let digits = format!("{high}{low}");
                let value = u8::from_str_radix(&digits, 16).map_err(|_| format!("Invalid hex escape `\\x{digits}`"))?;
                if value > 0x7F {
                    return Err("Non-ascii `\\xnn` escape is not valid in a rust UTF-8 string".to_string());
                }
                output.push(value as char);
            },
            | 'u' => {
                if chars.next() != Some('{') {
                    return Err("Unicode escapes must use `\\u{...}`".to_string());
                }
                let mut digits = String::new();
                loop {
                    match chars.next() {
                        | Some('}') => break,
                        | Some('_') => {},
                        | Some(value) => digits.push(value),
                        | None => return Err("Unterminated unicode escape".to_string()),
                    }
                }
                let value = u32::from_str_radix(&digits, 16).map_err(|_| "invalid Unicode escape".to_string())?;
                output.push(char::from_u32(value).ok_or_else(|| "Invalid unicode scalar".to_string())?);
            },
            | '\n' => {
                while matches!(chars.peek(), Some(character) if *character == ' ' || *character == '\t') {
                    chars.next();
                }
            },
            | other => return Err(format!("Unsupported rust string escape `\\{other}`")),
        }
    }
    Ok(output)
}

fn byte_array(bytes: &[u8]) -> TokenStream {
    parse_tokens(&byte_array_text(bytes))
}

fn parse_tokens(source: &str) -> TokenStream {
    TokenStream::from_str(source)
        .unwrap_or_else(|_| compile_errors(&["internal error: generated tokens were invalid".to_string()]))
}

fn byte_array_text(bytes: &[u8]) -> String {
    let mut output = String::from("[");
    for (index, byte) in bytes.iter().enumerate() {
        if index != 0 {
            output.push_str(", ");
        }
        write!(output, "0x{byte:02x}u8").expect("Writing to string cannot fail");
    }
    output.push(']');
    output
}

fn compile_errors(messages: &[String]) -> TokenStream {
    let mut output = String::new();
    for message in messages {
        writeln!(output, "compile_error!({message:?});").expect("Writing to string cannot fail");
    }
    TokenStream::from_str(&output).unwrap_or_else(|_| TokenStream::new())
}
