//! Shared fixtures for the `bvr-core` benchmarks.
//!
//! The benchmarks operate on a deterministic, synthetically generated log file
//! so that results are reproducible across machines and runs. The file is
//! cached under Cargo's `CARGO_TARGET_TMPDIR` and only regenerated when missing.
//!
//! The number of lines can be overridden with the `BVR_BENCH_LINES`
//! environment variable (default: 1,000,000 lines, roughly 90 MB).

#![allow(dead_code)]

use std::{
    fs::File,
    io::{BufWriter, Write},
    num::NonZeroUsize,
    path::PathBuf,
    sync::OnceLock,
};

use bvr_core::SegBuffer;

pub const DEFAULT_LINES: usize = 1_000_000;

/// Relative frequencies of each log level, out of 1000.
/// `ERROR` lines are rare, `INFO` lines are common.
const LEVELS: &[(&str, u32)] = &[("ERROR", 10), ("WARN", 50), ("INFO", 600), ("DEBUG", 340)];

const MODULES: &[&str] = &[
    "net::http",
    "net::tcp",
    "db::pool",
    "db::query",
    "auth",
    "cache",
    "scheduler",
    "storage::block",
];

const WORDS: &[&str] = &[
    "request", "response", "connection", "timeout", "retry", "latency", "bytes", "user",
    "session", "token", "handler", "worker", "queue", "flush", "commit", "rollback", "segment",
    "index", "volume", "snapshot", "replica", "leader", "follower", "heartbeat", "elapsed",
    "ok", "failed", "accepted", "rejected", "pending",
];

/// Small, fast, deterministic PRNG (xorshift64*). Good enough for fixture data.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

pub fn line_count() -> usize {
    std::env::var("BVR_BENCH_LINES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LINES)
}

fn write_log(path: &PathBuf, lines: usize) -> std::io::Result<()> {
    let tmp = path.with_extension("partial");
    let mut w = BufWriter::with_capacity(1 << 20, File::create(&tmp)?);
    let mut rng = Rng::new(0x5EED_B0B5);

    for i in 0..lines {
        let roll = rng.below(1000) as u32;
        let mut acc = 0;
        let level = LEVELS
            .iter()
            .find(|(_, w)| {
                acc += w;
                roll < acc
            })
            .map(|(l, _)| *l)
            .unwrap_or("INFO");
        let module = MODULES[rng.below(MODULES.len() as u64) as usize];

        let secs = i as u64 / 100;
        write!(
            w,
            "2026-10-09T{:02}:{:02}:{:02}.{:03}Z {:<5} [{}] id={:08x}",
            (secs / 3600) % 24,
            (secs / 60) % 60,
            secs % 60,
            rng.below(1000),
            level,
            module,
            rng.next_u64() as u32,
        )?;
        for _ in 0..4 + rng.below(12) {
            write!(w, " {}", WORDS[rng.below(WORDS.len() as u64) as usize])?;
        }
        w.write_all(b"\n")?;
    }

    w.flush()?;
    drop(w);
    std::fs::rename(tmp, path)
}

/// Returns the path to the cached fixture log, generating it if necessary.
pub fn log_path() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let lines = line_count();
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("bvr-bench");
        std::fs::create_dir_all(&dir).expect("create bench fixture dir");
        let path = dir.join(format!("synthetic_{lines}.log"));
        if !path.exists() {
            eprintln!("generating bench fixture {} ...", path.display());
            write_log(&path, lines).expect("write bench fixture");
        }
        path
    })
}

pub fn log_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| std::fs::read(log_path()).expect("read bench fixture"))
}

pub fn open_log() -> File {
    File::open(log_path()).expect("open bench fixture")
}

/// Opens the fixture as a [SegBuffer] and blocks until it is fully indexed.
pub fn loaded_buffer(seg_count: NonZeroUsize) -> SegBuffer {
    let buf = SegBuffer::read_file(open_log(), seg_count).expect("open SegBuffer");
    buf.wait_complete();
    buf
}

/// Generates a sorted, deduplicated set of line numbers in `0..universe`,
/// where each line is included with probability `density`.
pub fn sorted_lines(rng: &mut Rng, universe: usize, density: f64) -> Vec<usize> {
    let threshold = (density.clamp(0.0, 1.0) * u64::MAX as f64) as u64;
    (0..universe).filter(|_| rng.next_u64() <= threshold).collect()
}

/// Generates `n` random line numbers in `0..universe`.
pub fn random_lines(rng: &mut Rng, universe: usize, n: usize) -> Vec<usize> {
    (0..n).map(|_| rng.below(universe as u64) as usize).collect()
}
