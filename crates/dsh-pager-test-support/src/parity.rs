//! Shared validation for the checked-in M10 semantic parity manifest.

use std::fs;
use std::io;
use std::path::Path;

use serde::Deserialize;

const REQUIRED_SIZES: [[u16; 2]; 6] = [
    [40, 12],
    [60, 20],
    [80, 24],
    [100, 30],
    [120, 40],
    [160, 50],
];

#[derive(Debug, Clone, Deserialize)]
pub struct ParityManifest {
    pub status: String,
    #[serde(rename = "referenceMatrix")]
    pub reference_matrix: ReferenceMatrix,
    pub scenarios: Vec<ParityScenario>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReferenceMatrix {
    pub runner: String,
    pub status: String,
    pub sizes: Vec<[u16; 2]>,
    pub states: Vec<String>,
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParityScenario {
    pub name: String,
    pub fixture: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParityReport {
    pub case_count: usize,
    pub scenario_count: usize,
}

impl ParityManifest {
    pub fn from_path(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("parse parity manifest {}: {error}", path.display()),
            )
        })
    }

    pub fn validate(&self, fixture_root: impl AsRef<Path>) -> io::Result<ParityReport> {
        if self.status != "semantic-reference-v1" {
            return Err(invalid("manifest status is not semantic-reference-v1"));
        }
        if self.reference_matrix.status != self.status {
            return Err(invalid("reference matrix status does not match manifest"));
        }
        if self.reference_matrix.sizes != REQUIRED_SIZES {
            return Err(invalid("reference matrix dimensions drifted"));
        }
        if self.reference_matrix.states.len() < 12 || self.reference_matrix.inputs.len() < 9 {
            return Err(invalid(
                "reference matrix state/input coverage is incomplete",
            ));
        }
        let root = fixture_root.as_ref();
        for scenario in &self.scenarios {
            if !root.join(&scenario.fixture).is_file() {
                return Err(invalid(format!("missing fixture {}", scenario.fixture)));
            }
        }
        Ok(ParityReport {
            case_count: self.reference_matrix.sizes.len()
                * self.reference_matrix.states.len()
                * self.reference_matrix.inputs.len(),
            scenario_count: self.scenarios.len(),
        })
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_validation_counts_cases_and_fixtures() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/parity");
        let manifest = ParityManifest::from_path(root.join("manifest.json")).unwrap();
        let report = manifest.validate(&root).unwrap();
        assert_eq!(report.case_count, 972);
        assert_eq!(report.scenario_count, 8);
    }
}
