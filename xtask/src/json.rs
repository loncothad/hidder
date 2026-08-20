use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Result<&Json, String> {
        match self {
            | Self::Object(values) => values.get(key).ok_or_else(|| format!("Missing JSON key `{key}`")),
            | _ => Err(format!("Expected object while looking up `{key}`")),
        }
    }

    pub fn as_str(&self) -> Result<&str, String> {
        match self {
            | Self::String(value) => Ok(value),
            | _ => Err("Expected JSON string".to_string()),
        }
    }

    pub fn as_i64(&self) -> Result<i64, String> {
        match self {
            | Self::Number(value) => Ok(*value),
            | _ => Err("Expected JSON integer".to_string()),
        }
    }

    pub fn as_array(&self) -> Result<&[Json], String> {
        match self {
            | Self::Array(values) => Ok(values),
            | _ => Err("Expected JSON array".to_string()),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

pub fn parse(source: &str) -> Result<Json, String> {
    let mut parser = Parser {
        source:   source.as_bytes(),
        position: 0,
    };
    let value = parser.value()?;
    parser.whitespace();
    if parser.position != parser.source.len() {
        return Err(parser.error("trailing characters after JSON value"));
    }
    Ok(value)
}

struct Parser<'a> {
    source:   &'a [u8],
    position: usize,
}

impl Parser<'_> {
    fn value(&mut self) -> Result<Json, String> {
        self.whitespace();
        match self.peek() {
            | Some(b'n') => {
                self.keyword(b"null")?;
                Ok(Json::Null)
            },
            | Some(b't') => {
                self.keyword(b"true")?;
                Ok(Json::Bool(true))
            },
            | Some(b'f') => {
                self.keyword(b"false")?;
                Ok(Json::Bool(false))
            },
            | Some(b'"') => self.string().map(Json::String),
            | Some(b'[') => self.array(),
            | Some(b'{') => self.object(),
            | Some(b'-' | b'0' ..= b'9') => self.number().map(Json::Number),
            | Some(other) => Err(self.error(&format!("Unexpected byte {other:#04x}"))),
            | None => Err(self.error("unexpected end of JSON")),
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        self.whitespace();
        if self.consume(b']') {
            return Ok(Json::Array(values));
        }
        loop {
            values.push(self.value()?);
            self.whitespace();
            if self.consume(b']') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Array(values))
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut values = BTreeMap::new();
        self.whitespace();
        if self.consume(b'}') {
            return Ok(Json::Object(values));
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            self.whitespace();
            self.expect(b':')?;
            let value = self.value()?;
            if values.insert(key.clone(), value).is_some() {
                return Err(self.error(&format!("Duplicate object key `{key}`")));
            }
            self.whitespace();
            if self.consume(b'}') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Object(values))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut output = String::new();
        loop {
            let byte = self.next().ok_or_else(|| self.error("unterminated JSON string"))?;
            match byte {
                | b'"' => return Ok(output),
                | b'\\' => {
                    let escaped = self.next().ok_or_else(|| self.error("unterminated JSON escape"))?;
                    match escaped {
                        | b'"' => output.push('"'),
                        | b'\\' => output.push('\\'),
                        | b'/' => output.push('/'),
                        | b'b' => output.push('\u{0008}'),
                        | b'f' => output.push('\u{000c}'),
                        | b'n' => output.push('\n'),
                        | b'r' => output.push('\r'),
                        | b't' => output.push('\t'),
                        | b'u' => {
                            let first = self.hex_quad()?;
                            let scalar = if (0xD800 ..= 0xDBFF).contains(&first) {
                                self.expect(b'\\')?;
                                self.expect(b'u')?;
                                let second = self.hex_quad()?;
                                if !(0xDC00 ..= 0xDFFF).contains(&second) {
                                    return Err(self.error("invalid low surrogate"));
                                }
                                0x1_0000 + ((u32::from(first) - 0xD800) << 10) + (u32::from(second) - 0xDC00)
                            } else if (0xDC00 ..= 0xDFFF).contains(&first) {
                                return Err(self.error("unexpected low surrogate"));
                            } else {
                                u32::from(first)
                            };
                            output.push(char::from_u32(scalar).ok_or_else(|| self.error("invalid Unicode scalar"))?);
                        },
                        | _ => return Err(self.error("invalid JSON escape")),
                    }
                },
                | 0x00 ..= 0x1F => return Err(self.error("control byte in JSON string")),
                | 0x20 ..= 0x7F => output.push(byte as char),
                | _ => {
                    let start = self.position - 1;
                    let width = utf8_width(byte).ok_or_else(|| self.error("invalid UTF-8 leading byte"))?;
                    let end = start + width;
                    if end > self.source.len() {
                        return Err(self.error("truncated UTF-8 sequence"));
                    }
                    let text = std::str::from_utf8(&self.source[start .. end])
                        .map_err(|_| self.error("invalid UTF-8 sequence"))?;
                    output.push_str(text);
                    self.position = end;
                },
            }
        }
    }

    fn hex_quad(&mut self) -> Result<u16, String> {
        let mut value = 0_u16;
        for _ in 0 .. 4 {
            let byte = self.next().ok_or_else(|| self.error("truncated Unicode escape"))?;
            let digit = match byte {
                | b'0' ..= b'9' => u16::from(byte - b'0'),
                | b'a' ..= b'f' => u16::from(byte - b'a' + 10),
                | b'A' ..= b'F' => u16::from(byte - b'A' + 10),
                | _ => return Err(self.error("invalid Unicode escape digit")),
            };
            value = (value << 4) | digit;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<i64, String> {
        let start = self.position;
        self.consume(b'-');
        match self.peek() {
            | Some(b'0') => {
                self.position += 1;
            },
            | Some(b'1' ..= b'9') => {
                while matches!(self.peek(), Some(b'0' ..= b'9')) {
                    self.position += 1;
                }
            },
            | _ => return Err(self.error("invalid JSON number")),
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return Err(self.error("floating-point JSON numbers are not supported by this schema"));
        }
        std::str::from_utf8(&self.source[start .. self.position])
            .map_err(|_| self.error("invalid number encoding"))?
            .parse()
            .map_err(|_| self.error("integer outside i64 range"))
    }

    fn keyword(&mut self, keyword: &[u8]) -> Result<(), String> {
        if self.source.get(self.position .. self.position + keyword.len()) == Some(keyword) {
            self.position += keyword.len();
            Ok(())
        } else {
            Err(self.error("invalid JSON keyword"))
        }
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.position += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        self.whitespace();
        if self.consume(byte) {
            Ok(())
        } else {
            Err(self.error(&format!("Expected `{}`", byte as char)))
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.source.get(self.position).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.position += 1;
        Some(value)
    }

    fn error(&self, message: &str) -> String {
        format!("JSON byte {}: {message}", self.position)
    }
}

fn utf8_width(first: u8) -> Option<usize> {
    match first {
        | 0xC2 ..= 0xDF => Some(2),
        | 0xE0 ..= 0xEF => Some(3),
        | 0xF0 ..= 0xF4 => Some(4),
        | _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unicode_and_nested_values() {
        let value = parse(r#"{"a":[1,"Moir\u00e9",null,true]}"#).unwrap();
        assert_eq!(value.get("a").unwrap().as_array().unwrap().len(), 4);
    }
}
