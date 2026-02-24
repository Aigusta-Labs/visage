use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Model file descriptor: URL, expected filename, SHA-256 checksum, human-readable size.
pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_display: &'static str,
}

// Checksums verified from HuggingFace Git LFS pointer files (oid sha256: field).
// Source: https://huggingface.co/public-data/insightface/raw/main/models/buffalo_l/
pub const MODELS: &[ModelFile] = &[
    ModelFile {
        name: "det_10g.onnx",
        url: "https://huggingface.co/public-data/insightface/resolve/main/models/buffalo_l/det_10g.onnx",
        sha256: "5838f7fe053675b1c7a08b633df49e7af5495cee0493c7dcf6697200b85b5b91",
        size_display: "16 MB",
    },
    ModelFile {
        name: "w600k_r50.onnx",
        url: "https://huggingface.co/public-data/insightface/resolve/main/models/buffalo_l/w600k_r50.onnx",
        sha256: "4c06341c33c2ca1f86781dab0e829f88ad5b64be9fba56e56bc9ebdefc619e43",
        size_display: "166 MB",
    },
];

#[derive(Error, Debug)]
pub enum ModelIntegrityError {
    #[error("model file not found: {name} ({path})")]
    MissingModel { name: &'static str, path: PathBuf },

    #[error("failed to open model file: {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read model file: {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "model checksum mismatch for {name} ({path})\n  expected: {expected}\n  got:      {got}"
    )]
    ChecksumMismatch {
        name: &'static str,
        path: PathBuf,
        expected: String,
        got: String,
    },
}

/// Compute SHA-256 hex digest of a file.
pub fn sha256_file_hex(path: &Path) -> Result<String, ModelIntegrityError> {
    let mut file = fs::File::open(path).map_err(|source| ModelIntegrityError::Open {
        path: path.to_path_buf(),
        source,
    })?;

    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = file
            .read(&mut buf)
            .map_err(|source| ModelIntegrityError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verify_file_sha256(
    name: &'static str,
    path: &Path,
    expected_sha256: &str,
) -> Result<(), ModelIntegrityError> {
    if !path.exists() {
        return Err(ModelIntegrityError::MissingModel {
            name,
            path: path.to_path_buf(),
        });
    }

    let digest = sha256_file_hex(path)?;
    if digest != expected_sha256 {
        return Err(ModelIntegrityError::ChecksumMismatch {
            name,
            path: path.to_path_buf(),
            expected: expected_sha256.to_string(),
            got: digest,
        });
    }

    Ok(())
}

pub fn verify_models_dir(model_dir: &Path) -> Result<(), ModelIntegrityError> {
    for model in MODELS {
        let path = model_dir.join(model.name);
        verify_file_sha256(model.name, &path, model.sha256)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_file_sha256_rejects_missing() {
        let tmp = std::env::temp_dir().join(format!(
            "visage-models-test-missing-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = tmp.join("nope.onnx");

        let err = verify_file_sha256("nope.onnx", &path, "00").unwrap_err();
        assert!(matches!(err, ModelIntegrityError::MissingModel { .. }));
    }

    #[test]
    fn verify_file_sha256_rejects_mismatch() {
        let dir = std::env::temp_dir().join(format!(
            "visage-models-test-mismatch-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("model.onnx");
        fs::write(&path, b"hello").unwrap();

        let err = verify_file_sha256("model.onnx", &path, "00").unwrap_err();
        assert!(matches!(err, ModelIntegrityError::ChecksumMismatch { .. }));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_file_sha256_accepts_match() {
        let dir = std::env::temp_dir().join(format!(
            "visage-models-test-match-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("model.onnx");
        fs::write(&path, b"hello").unwrap();

        let digest = sha256_file_hex(&path).unwrap();
        verify_file_sha256("model.onnx", &path, &digest).unwrap();

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_models_dir_reports_missing() {
        let dir = std::env::temp_dir().join(format!(
            "visage-models-test-dir-missing-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let err = verify_models_dir(&dir).unwrap_err();
        assert!(matches!(err, ModelIntegrityError::MissingModel { .. }));
    }
}
