use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

pub fn hash_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();

        let mut f1 = fs::File::create(dir.path().join("a.txt")).unwrap();
        f1.write_all(b"hello world").unwrap();

        let mut f2 = fs::File::create(dir.path().join("b.txt")).unwrap();
        f2.write_all(b"hello world").unwrap();

        let mut f3 = fs::File::create(dir.path().join("c.txt")).unwrap();
        f3.write_all(b"different content here").unwrap();

        dir
    }

    #[test]
    fn test_hash_file_identical_contents() {
        let dir = setup_test_dir();
        let hash_a = hash_file(&dir.path().join("a.txt")).unwrap();
        let hash_b = hash_file(&dir.path().join("b.txt")).unwrap();
        assert_eq!(hash_a, hash_b, "identical files should have same hash");
    }

    #[test]
    fn test_hash_file_different_contents() {
        let dir = setup_test_dir();
        let hash_a = hash_file(&dir.path().join("a.txt")).unwrap();
        let hash_c = hash_file(&dir.path().join("c.txt")).unwrap();
        assert_ne!(
            hash_a, hash_c,
            "different files should have different hashes"
        );
    }

    #[test]
    fn test_hash_file_returns_64_hex_chars() {
        let dir = setup_test_dir();
        let hash = hash_file(&dir.path().join("a.txt")).unwrap();
        assert_eq!(hash.len(), 64, "SHA-256 hex digest must be 64 chars");
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1572864), "1.50 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1073741824), "1.00 GB");
        assert_eq!(format_size(1610612736), "1.50 GB");
    }
}
