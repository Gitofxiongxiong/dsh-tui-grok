use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A named JSONL fixture. Every line is parsed independently so malformed
/// lines report their one-based location instead of failing opaquely later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonlFixture {
    pub name: String,
    pub records: Vec<Value>,
}

impl JsonlFixture {
    pub fn new(name: impl Into<String>, records: Vec<Value>) -> Self {
        Self {
            name: name.into(),
            records,
        }
    }

    pub fn from_str(name: impl Into<String>, text: &str) -> io::Result<Self> {
        let name = name.into();
        let mut records = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let value = serde_json::from_str(line).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{name} JSONL line {}: {error}", index + 1),
                )
            })?;
            records.push(value);
        }
        Ok(Self { name, records })
    }

    pub fn to_jsonl(&self) -> io::Result<String> {
        let mut output = String::new();
        for record in &self.records {
            let text = serde_json::to_string(record).map_err(|error| {
                io::Error::new(io::ErrorKind::InvalidData, format!("encode JSONL: {error}"))
            })?;
            output.push_str(&text);
            output.push('\n');
        }
        Ok(output)
    }
}

pub fn read_jsonl(path: impl AsRef<Path>) -> io::Result<JsonlFixture> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)?;
    JsonlFixture::from_str(path.display().to_string(), &text)
}

pub fn write_jsonl(path: impl AsRef<Path>, fixture: &JsonlFixture) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, fixture.to_jsonl()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonl_round_trip_ignores_comments_and_blank_lines() {
        let fixture = JsonlFixture::from_str(
            "fixture",
            "# header\n\n{\"seq\":0}\n{\"seq\":1,\"text\":\"two\"}\n",
        )
        .expect("valid JSONL");
        assert_eq!(fixture.records.len(), 2);
        let encoded = fixture.to_jsonl().expect("encode JSONL");
        let round_trip = JsonlFixture::from_str("round-trip", &encoded).unwrap();
        assert_eq!(round_trip.records, fixture.records);
    }

    #[test]
    fn malformed_jsonl_reports_line_number() {
        let error = JsonlFixture::from_str("fixture", "{\"ok\":true}\nnot-json\n")
            .expect_err("malformed line must fail");
        assert!(error.to_string().contains("line 2"));
    }
}
