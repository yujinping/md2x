use std::fmt;
use std::io;

#[derive(Debug)]
pub enum MpeError {
    FileNotFound(String),
    ChromeNotFound,
    PdfGenerationFailed(String),
    IoError(io::Error),
}

impl fmt::Display for MpeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MpeError::FileNotFound(path) => write!(f, "File not found: {path}"),
            MpeError::ChromeNotFound => {
                write!(f, "Chrome not found. Please install Google Chrome")
            }
            MpeError::PdfGenerationFailed(msg) => write!(f, "PDF generation failed: {msg}"),
            MpeError::IoError(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for MpeError {}

impl From<io::Error> for MpeError {
    fn from(e: io::Error) -> Self {
        MpeError::IoError(e)
    }
}
