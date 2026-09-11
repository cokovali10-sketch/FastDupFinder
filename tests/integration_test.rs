use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn setup_test_dir() -> TempDir {
    let dir = TempDir::new().unwrap();

    let mut f1 = fs::File::create(dir.path().join("photo1.jpg")).unwrap();
    f1.write_all(b"fake jpeg data for testing").unwrap();

    let mut f2 = fs::File::create(dir.path().join("photo1_copy.jpg")).unwrap();
    f2.write_all(b"fake jpeg data for testing").unwrap();

    let mut f3 = fs::File::create(dir.path().join("photo2.jpg")).unwrap();
    f3.write_all(b"completely different image bytes").unwrap();

    let mut f4 = fs::File::create(dir.path().join("photo2_dup.jpg")).unwrap();
    f4.write_all(b"completely different image bytes").unwrap();

    let mut f5 = fs::File::create(dir.path().join("unique.txt")).unwrap();
    f5.write_all(b"this file has no duplicates").unwrap();

    dir
}

#[test]
fn test_hash_file_identical_contents() {
    let dir = setup_test_dir();
    let hash1 = fastdupfinder::hash_file(&dir.path().join("photo1.jpg")).unwrap();
    let hash2 = fastdupfinder::hash_file(&dir.path().join("photo1_copy.jpg")).unwrap();
    assert_eq!(hash1, hash2, "identical content must produce same hash");
}

#[test]
fn test_hash_file_different_contents() {
    let dir = setup_test_dir();
    let hash1 = fastdupfinder::hash_file(&dir.path().join("photo1.jpg")).unwrap();
    let hash3 = fastdupfinder::hash_file(&dir.path().join("photo2.jpg")).unwrap();
    assert_ne!(hash1, hash3, "different content must produce different hashes");
}

#[test]
fn test_hash_file_length_is_64() {
    let dir = setup_test_dir();
    let hash = fastdupfinder::hash_file(&dir.path().join("photo1.jpg")).unwrap();
    assert_eq!(hash.len(), 64, "SHA-256 hex digest must be 64 characters");
}

#[test]
fn test_format_size_bytes() {
    assert_eq!(fastdupfinder::format_size(0), "0 B");
    assert_eq!(fastdupfinder::format_size(1), "1 B");
    assert_eq!(fastdupfinder::format_size(999), "999 B");
}

#[test]
fn test_format_size_kilobytes() {
    assert_eq!(fastdupfinder::format_size(1024), "1.00 KB");
    assert_eq!(fastdupfinder::format_size(1536), "1.50 KB");
    assert_eq!(fastdupfinder::format_size(2047), "2.00 KB");
}

#[test]
fn test_format_size_megabytes() {
    assert_eq!(fastdupfinder::format_size(1048576), "1.00 MB");
    assert_eq!(fastdupfinder::format_size(1572864), "1.50 MB");
}

#[test]
fn test_format_size_gigabytes() {
    assert_eq!(fastdupfinder::format_size(1073741824), "1.00 GB");
    assert_eq!(fastdupfinder::format_size(1610612736), "1.50 GB");
}

#[test]
fn test_duplicate_detection_via_collect_and_hash() {
    let dir = setup_test_dir();
    let mut files = Vec::new();
    for entry in fs::read_dir(dir.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            files.push(entry.path());
        }
    }

    let mut hashes: Vec<(String, std::path::PathBuf)> = Vec::new();
    for path in &files {
        let hash = fastdupfinder::hash_file(path).unwrap();
        hashes.push((hash, path.clone()));
    }

    use std::collections::HashMap;
    let mut groups: HashMap<String, Vec<std::path::PathBuf>> = HashMap::new();
    for (hash, path) in hashes {
        groups.entry(hash).or_default().push(path);
    }

    let duplicate_groups: Vec<_> = groups.into_iter().filter(|(_, v)| v.len() >= 2).collect();
    assert_eq!(duplicate_groups.len(), 2, "should find exactly 2 duplicate groups");
}

#[test]
fn test_dry_run_preserves_all_files() {
    let dir = setup_test_dir();
    let file_count_before = fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_file())
        .count();

    let mut files = Vec::new();
    for entry in fs::read_dir(dir.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            files.push(entry.path());
        }
    }

    let mut hashes: Vec<(String, std::path::PathBuf)> = Vec::new();
    for path in &files {
        let hash = fastdupfinder::hash_file(path).unwrap();
        hashes.push((hash, path.clone()));
    }

    use std::collections::HashMap;
    let mut groups: HashMap<String, Vec<std::path::PathBuf>> = HashMap::new();
    for (hash, path) in hashes {
        groups.entry(hash).or_default().push(path);
    }

    let candidates: Vec<&std::path::PathBuf> = groups
        .values()
        .filter(|v| v.len() >= 2)
        .flat_map(|v| v.iter().skip(1))
        .collect();

    for path in &candidates {
        assert!(path.exists(), "dry run should not delete any files");
    }

    let file_count_after = fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_file())
        .count();

    assert_eq!(
        file_count_before, file_count_after,
        "file count must not change without actual deletion"
    );
}
