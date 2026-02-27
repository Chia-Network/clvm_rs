use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use clvmr::allocator::Allocator;
use clvmr::serde::{
    node_from_bytes, node_from_bytes_backrefs, node_to_bytes_backrefs, node_to_bytes_limit,
};
use clvmr::serde_2026::{deserialize_2026, serialize_2026};

#[derive(Parser)]
#[command(about = "Compare legacy, backref, and 2026 serialization formats")]
struct Args {
    /// .generator files to benchmark (default: benches/[0-4].generator)
    files: Vec<PathBuf>,

    /// Number of iterations for timing
    #[arg(short, long, default_value_t = 10)]
    iterations: usize,
}

struct FormatResult {
    name: &'static str,
    size: usize,
    ser_us: f64,
    deser_us: f64,
}

fn main() {
    let args = Args::parse();

    let files: Vec<PathBuf> = if args.files.is_empty() {
        (0..=4)
            .map(|i| PathBuf::from(format!("benches/{i}.generator")))
            .filter(|p| p.exists())
            .collect()
    } else {
        args.files
    };

    if files.is_empty() {
        eprintln!("No .generator files found.");
        std::process::exit(1);
    }

    let iters = args.iterations;

    for path in &files {
        let raw = std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("Failed to read {}: {e}", path.display());
            std::process::exit(1);
        });

        let mut a = Allocator::new();
        let node = node_from_bytes_backrefs(&mut a, &raw).expect("node_from_bytes_backrefs");

        // Inflate to uncompressed form (cap at 10MB to avoid blowup)
        let (a, node) = if let Ok(inflated) = node_to_bytes_limit(&a, node, 10_000_000) {
            let mut a2 = Allocator::new();
            let n = node_from_bytes(&mut a2, &inflated).expect("node_from_bytes");
            (a2, n)
        } else {
            (a, node)
        };

        let mut results: Vec<FormatResult> = Vec::new();

        // --- Legacy (no backrefs) ---
        if let Ok(serialized) = node_to_bytes_limit(&a, node, 50_000_000) {
            let size = serialized.len();

            let start = Instant::now();
            for _ in 0..iters {
                let _ = node_to_bytes_limit(&a, node, 50_000_000).unwrap();
            }
            let ser_us = start.elapsed().as_micros() as f64 / iters as f64;

            let start = Instant::now();
            for _ in 0..iters {
                let mut a2 = Allocator::new();
                let _ = node_from_bytes(&mut a2, &serialized).unwrap();
            }
            let deser_us = start.elapsed().as_micros() as f64 / iters as f64;

            results.push(FormatResult {
                name: "legacy",
                size,
                ser_us,
                deser_us,
            });
        } else {
            eprintln!("  (legacy serialization too large, skipped)");
        }

        // --- Backrefs (compressed) ---
        {
            let serialized = node_to_bytes_backrefs(&a, node).expect("node_to_bytes_backrefs");
            let size = serialized.len();

            let start = Instant::now();
            for _ in 0..iters {
                let _ = node_to_bytes_backrefs(&a, node).unwrap();
            }
            let ser_us = start.elapsed().as_micros() as f64 / iters as f64;

            let start = Instant::now();
            for _ in 0..iters {
                let mut a2 = Allocator::new();
                let _ = node_from_bytes_backrefs(&mut a2, &serialized).unwrap();
            }
            let deser_us = start.elapsed().as_micros() as f64 / iters as f64;

            results.push(FormatResult {
                name: "backrefs",
                size,
                ser_us,
                deser_us,
            });
        }

        // --- 2026 ---
        {
            let serialized = serialize_2026(&a, node).expect("serialize_2026");
            let size = serialized.len();

            let start = Instant::now();
            for _ in 0..iters {
                let _ = serialize_2026(&a, node).unwrap();
            }
            let ser_us = start.elapsed().as_micros() as f64 / iters as f64;

            let start = Instant::now();
            for _ in 0..iters {
                let mut a2 = Allocator::new();
                let _ = deserialize_2026(&mut a2, &serialized, None).unwrap();
            }
            let deser_us = start.elapsed().as_micros() as f64 / iters as f64;

            results.push(FormatResult {
                name: "2026",
                size,
                ser_us,
                deser_us,
            });
        }

        println!("=== {} ===", path.display());
        println!(
            "  {:>10}  {:>12}  {:>12}  {:>10}",
            "format", "ser (µs)", "deser (µs)", "size"
        );
        for r in &results {
            println!(
                "  {:>10}  {:>12.1}  {:>12.1}  {:>10}",
                r.name, r.ser_us, r.deser_us, r.size
            );
        }
        let legacy_size = results.iter().find(|r| r.name == "legacy").map(|r| r.size);
        if let Some(base) = legacy_size {
            if results.len() >= 2 {
                println!();
                println!("  size vs legacy:");
                for r in results.iter().filter(|r| r.name != "legacy") {
                    let ratio = r.size as f64 / base as f64 * 100.0;
                    println!("    {}: {:.1}%", r.name, ratio);
                }
            }
        }
        println!();
    }
}
