use clap::{Parser, ValueEnum};
use csv::Writer;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use walkdir::WalkDir;

use fastdupfinder::{format_size, hash_file};

#[derive(Parser)]
#[command(
    name = "fastdupfinder",
    version,
    about = "Blazing-fast duplicate file finder using two-pass detection",
    long_about = "FastDupFinder finds duplicate files using a two-pass algorithm:\n\n\
        1. PASS 1 — Size pre-filter: Groups files by size, discarding unique sizes instantly.\n\
        2. PASS 2 — SHA-256 hash: Only hashes files that share a size, then groups by hash.\n\n\
        This avoids hashing millions of files and is orders of magnitude faster than naive approaches."
)]
struct Cli {
    /// Directory to scan for duplicates
    #[arg(default_value = ".")]
    directory: String,

    /// Maximum file size in bytes (files larger than this are skipped)
    #[arg(short = 'M', long, default_value = "104857600")]
    max_size: u64,

    /// Minimum file size in bytes (files smaller than this are skipped)
    #[arg(short, long)]
    min_size: Option<u64>,

    /// Delete duplicate files (keeps the first occurrence in each group)
    #[arg(short, long)]
    delete: bool,

    /// Preview what --delete would do without actually removing files
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Output format
    #[arg(short = 'f', long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    /// Output file path (prints to stdout if omitted)
    #[arg(short = 'o', long)]
    output: Option<String>,

    /// Disable progress bar
    #[arg(long)]
    no_progress: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Csv,
}

#[derive(Serialize)]
struct DuplicationGroup {
    hash: String,
    size_bytes: u64,
    size_human: String,
    count: usize,
    wasted_bytes: u64,
    wasted_human: String,
    files: Vec<String>,
}

#[derive(Serialize)]
struct ScanResult {
    directory: String,
    total_files_scanned: u64,
    total_size_scanned: u64,
    duplicate_groups: usize,
    total_duplicates: usize,
    total_wasted_bytes: u64,
    total_wasted_human: String,
    elapsed_ms: u64,
    groups: Vec<DuplicationGroup>,
}

fn collect_files(dir: &std::path::Path, cli: &Cli) -> (Vec<PathBuf>, u64, u64) {
    let mut files = Vec::new();
    let mut file_count = 0u64;
    let mut total_size = 0u64;

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
        total_size += size;

        if size == 0 || size > cli.max_size {
            continue;
        }
        if let Some(min) = cli.min_size {
            if size < min {
                continue;
            }
        }

        files.push(path.to_path_buf());
    }

    (files, file_count, total_size)
}

fn find_duplicates(
    files: &[PathBuf],
    pb: &Option<ProgressBar>,
) -> Vec<(String, u64, Vec<PathBuf>)> {
    let mut size_groups: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for path in files {
        if let Ok(metadata) = fs::metadata(path) {
            size_groups
                .entry(metadata.len())
                .or_default()
                .push(path.clone());
        }
    }

    let hash_map: Arc<Mutex<HashMap<String, (u64, Vec<PathBuf>)>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let counter = Arc::new(Mutex::new(0u64));
    let total_files: usize = size_groups
        .values()
        .filter(|v| v.len() >= 2)
        .map(|v| v.len())
        .sum();

    size_groups
        .par_iter()
        .map(|(_, paths)| paths)
        .filter(|paths| paths.len() >= 2)
        .for_each(|paths| {
            let mut local_hashes: Vec<(String, PathBuf)> = Vec::new();
            for path in paths {
                if let Ok(hash) = hash_file(path) {
                    local_hashes.push((hash, path.clone()));
                }
            }

            let mut guard = hash_map.lock().unwrap();
            for (hash, path) in local_hashes {
                let entry = guard.entry(hash).or_insert_with(|| {
                    let size = paths[0].metadata().map(|m| m.len()).unwrap_or(0);
                    (size, Vec::new())
                });
                entry.1.push(path);
            }

            let mut count = counter.lock().unwrap();
            *count += 1;
            if let Some(ref pb) = pb {
                pb.inc(1);
                if *count % 50 == 0 || *count as usize == total_files {
                    pb.set_message(format!("hashed {}/{} files", count, total_files));
                }
            }
        });

    let results: Vec<(String, u64, Vec<PathBuf>)> = hash_map
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, (_, paths))| paths.len() >= 2)
        .map(|(hash, (size, paths))| (hash.clone(), *size, paths.clone()))
        .collect();

    results
}

fn write_text_output(
    results: &[(String, u64, Vec<PathBuf>)],
    total_wasted: u64,
    total_files: u64,
    total_size: u64,
    dir: &str,
    elapsed_ms: u64,
) {
    println!(
        "Scan complete in {:.2?}\n",
        std::time::Duration::from_millis(elapsed_ms)
    );
    println!("Directory:     {}", dir);
    println!("Files scanned: {}", total_files);
    println!("Total size:    {}\n", format_size(total_size));

    if results.is_empty() {
        println!("No duplicate files found.");
        return;
    }

    for (hash, size, group) in results {
        let wasted = size * (group.len() as u64 - 1);
        println!(
            "Group [{}..] ({} bytes, {} copies, wasted {}):",
            &hash[..8],
            size,
            group.len(),
            format_size(wasted)
        );
        for path in group {
            println!("  {}", path.display());
        }
        println!();
    }

    let mut total_dupes = 0usize;
    for (_, _, group) in results {
        total_dupes += group.len() - 1;
    }

    println!("Summary:");
    println!("  Duplicate groups:  {}", results.len());
    println!("  Duplicate files:   {}", total_dupes);
    println!("  Wasted space:      {}", format_size(total_wasted));
    println!(
        "  Scan duration:     {:.2?}",
        std::time::Duration::from_millis(elapsed_ms)
    );
}

fn write_json_output(
    results: &[(String, u64, Vec<PathBuf>)],
    total_files: u64,
    total_size: u64,
    total_wasted: u64,
    dir: &str,
    elapsed_ms: u64,
    output_path: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let groups: Vec<DuplicationGroup> = results
        .iter()
        .map(|(hash, size, paths)| {
            let wasted = *size * (paths.len() as u64 - 1);
            DuplicationGroup {
                hash: hash.clone(),
                size_bytes: *size,
                size_human: format_size(*size),
                count: paths.len(),
                wasted_bytes: wasted,
                wasted_human: format_size(wasted),
                files: paths.iter().map(|p| p.display().to_string()).collect(),
            }
        })
        .collect();

    let result = ScanResult {
        directory: dir.to_string(),
        total_files_scanned: total_files,
        total_size_scanned: total_size,
        duplicate_groups: groups.len(),
        total_duplicates: groups.iter().map(|g| g.count - 1).sum(),
        total_wasted_bytes: total_wasted,
        total_wasted_human: format_size(total_wasted),
        elapsed_ms,
        groups,
    };

    let json = serde_json::to_string_pretty(&result)?;
    match output_path {
        Some(path) => fs::write(path, &json)?,
        None => println!("{}", json),
    }
    Ok(())
}

fn write_csv_output(
    results: &[(String, u64, Vec<PathBuf>)],
    output_path: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    match output_path {
        Some(path) => {
            let mut wtr = Writer::from_path(path)?;
            wtr.write_record(["hash", "size_bytes", "size_human", "group_index", "file"])?;
            for (hash, size, paths) in results {
                for (i, path) in paths.iter().enumerate() {
                    wtr.write_record([
                        hash,
                        &size.to_string(),
                        &format_size(*size),
                        &(i + 1).to_string(),
                        &path.display().to_string(),
                    ])?;
                }
            }
            wtr.flush()?;
        }
        None => {
            let mut wtr = Writer::from_writer(std::io::stdout());
            wtr.write_record(["hash", "size_bytes", "size_human", "group_index", "file"])?;
            for (hash, size, paths) in results {
                for (i, path) in paths.iter().enumerate() {
                    wtr.write_record([
                        hash,
                        &size.to_string(),
                        &format_size(*size),
                        &(i + 1).to_string(),
                        &path.display().to_string(),
                    ])?;
                }
            }
            wtr.flush()?;
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let dir = std::path::Path::new(&cli.directory);

    if !dir.exists() {
        eprintln!("Error: directory '{}' does not exist", cli.directory);
        std::process::exit(1);
    }

    let start = Instant::now();
    eprintln!("Scanning '{}' for duplicate files...\n", cli.directory);

    let (files, file_count, total_size) = collect_files(dir, &cli);
    eprintln!(
        "Found {} eligible files (of {} total)\n",
        files.len(),
        file_count
    );

    let pb = if cli.no_progress {
        None
    } else {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg} ({pos}/{len})",
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        Some(pb)
    };

    let results = find_duplicates(&files, &pb);
    if let Some(ref pb) = pb {
        pb.finish_with_message("done");
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;

    let mut total_wasted = 0u64;
    for (_, size, group) in &results {
        total_wasted += *size * (group.len() as u64 - 1);
    }

    if cli.delete && !results.is_empty() {
        let to_delete: Vec<&PathBuf> = results
            .iter()
            .flat_map(|(_, _, group)| group.iter().skip(1))
            .collect();

        if cli.dry_run {
            eprintln!("\n[DRY RUN] Would delete {} files:\n", to_delete.len());
            for path in &to_delete {
                eprintln!("  {}", path.display());
            }
        } else {
            eprintln!("\nDeleting {} duplicate files...\n", to_delete.len());
            let pb_del = ProgressBar::new(to_delete.len() as u64);
            for path in &to_delete {
                match fs::remove_file(path) {
                    Ok(_) => pb_del.inc(1),
                    Err(e) => eprintln!("  Failed to delete {}: {}", path.display(), e),
                }
            }
            pb_del.finish_with_message("deletion complete");
        }
    }

    match cli.format {
        OutputFormat::Text => write_text_output(
            &results,
            total_wasted,
            file_count,
            total_size,
            &cli.directory,
            elapsed_ms,
        ),
        OutputFormat::Json => write_json_output(
            &results,
            file_count,
            total_size,
            total_wasted,
            &cli.directory,
            elapsed_ms,
            &cli.output,
        )?,
        OutputFormat::Csv => write_csv_output(&results, &cli.output)?,
    }

    Ok(())
}
