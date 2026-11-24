use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize)]
pub enum Error {
    Missing { call: String, path: String, line: u32 },
}

impl Error {
    pub fn line(&self) -> u32 {
        match self {
            Self::Missing { line, .. } => *line,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Missing { call, path, line } if call == "font" && crate::bare(path) => write!(
                f,
                "line {line}: font(\"{path}\") is not a font installed on this Mac, or a font file next to the script"
            ),
            Self::Missing { call, path, line } => write!(f, "line {line}: {call}(\"{path}\") points to a file that does not exist"),
        }
    }
}

impl std::error::Error for Error {}
