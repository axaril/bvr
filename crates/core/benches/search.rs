//! Benchmarks for regex searching over a fully indexed buffer.

mod common;

use std::hint::black_box;

use bvr_core::{LineSet, SegBuffer};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use regex::bytes::Regex;

/// Patterns chosen to cover a range of match densities and regex engine paths.
const PATTERNS: &[(&str, &str)] = &[
    // ~1% of lines, simple literal (memchr/prefilter fast path).
    ("rare_literal", "ERROR"),
    // ~60% of lines, so the cost is dominated by per-match bookkeeping.
    ("common_literal", "INFO"),
    // Never matches; measures raw scan throughput.
    ("no_match", "zzqqxj"),
    ("alternation", "timeout|rollback|rejected"),
    ("case_insensitive", "(?i)heartbeat"),
    ("complex", r"\[db::\w+\] id=[0-9a-f]{2}ff.*(failed|rejected)"),
];

fn bench_search(c: &mut Criterion) {
    let buf = common::loaded_buffer(SegBuffer::SEG_COUNT);

    let mut group = c.benchmark_group("search");
    group.throughput(Throughput::Bytes(common::log_bytes().len() as u64));
    group.sample_size(10);

    for &(name, pattern) in PATTERNS {
        let regex = Regex::new(pattern).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(name), &regex, |b, regex| {
            b.iter(|| {
                let set = LineSet::search(buf.segment_iter().unwrap(), regex.clone());
                set.wait_for_index(usize::MAX);
                black_box(set.len())
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
