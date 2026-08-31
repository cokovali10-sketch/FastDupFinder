use clap::Parser;
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "fastdupfinder", about = "Fast duplicate file finder")]
struct Cli {
    #[arg(default_value = ".")]
    directory: String,

    #[arg(short, long, default_value = "1048576")]
    max_size: u64,

    #[arg(short, long)]
    min_size: Option<u64>,

    #[arg(short, long)]
    delete: bool,

    #[arg(short, long)]
    dry_run: bool,
}

fn hash_file(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
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

fn format_size(bytes: u64) -> String {
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let dir = std::path::Path::new(&cli.directory);

    if !dir.exists() {
        eprintln!("Error: directory '{}' does not exist", cli.directory);
        std::process::exit(1);
    }

    eprintln!("Scanning '{}' for duplicate files...\n", cli.directory);

    let mut size_groups: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut file_count = 0u64;

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let size = metadata.len();
        file_count += 1;

        if size == 0 || size > cli.max_size {
            continue;
        }
        if let Some(min) = cli.min_size {
            if size < min {
                continue;
            }
        }

        size_groups.entry(size).or_default().push(path.to_path_buf());
    }

    eprintln!("Scanned {} files\n", file_count);

    let mut duplicates: Vec<(u64, Vec<PathBuf>)> = Vec::new();

    for (size, paths) in &size_groups {
        if paths.len() < 2 {
            continue;
        }

        let mut hash_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for path in paths {
            if let Ok(hash) = hash_file(path) {
                hash_groups.entry(hash).or_default().push(path.clone());
            }
        }

        for (_hash, group) in hash_groups {
            if group.len() >= 2 {
                duplicates.push((*size, group));
            }
        }
    }

    duplicates.sort_by(|a, b| b.0.cmp(&a.0));

    if duplicates.is_empty() {
        println!("No duplicate files found.");
        return Ok(());
    }

    let mut total_wasted = 0u64;

    for (size, group) in &duplicates {
        let wasted = size * (group.len() as u64 - 1);
        total_wasted += wasted;

        println!("Hash group ({} bytes each, {} copies):", size, group.len());
        println!("  Wasted: {}", format_size(wasted));
        for path in group {
            println!("  {}", path.display());
        }

        if cli.delete && group.len() > 1 {
            let keep = &group[0];
            for path in group.iter().skip(1) {
                if cli.dry_run {
                    println!("  [DRY RUN] Would delete: {}", path.display());
                } else {
                    match fs::remove_file(path) {
                        Ok(_) => println!("  Deleted: {}", path.display()),
                        Err(e) => eprintln!("  Failed to delete {}: {}", path.display(), e),
                    }
                }
            }
            println!("  Kept: {}", keep.display());
        }
        println!();
    }

    println!("Summary:");
    println!("  Duplicate groups: {}", duplicates.len());
    println!("  Total wasted space: {}", format_size(total_wasted));

    Ok(())
}
