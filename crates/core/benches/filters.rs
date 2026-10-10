//! Benchmarks for merging (composing) filters with union and intersection.

mod common;

use std::hint::black_box;

use bvr_core::{matches::CompositeStrategy, LineSet, SegBuffer};
use common::Rng;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use regex::bytes::Regex;

const STRATEGIES: &[(&str, CompositeStrategy)] = &[
    ("union", CompositeStrategy::Union),
    ("intersection", CompositeStrategy::Intersection),
];

fn compose(filters: &[LineSet], strategy: CompositeStrategy) -> usize {
    // Cloning a `LineSet` only bumps an `Arc`, so this is negligible.
    let set = LineSet::compose(filters.to_vec(), true, strategy).unwrap();
    set.len()
}

/// Composition of already-complete, in-memory line sets. This isolates the
/// cost of the k-way merge itself.
fn bench_compose_static(c: &mut Criterion) {
    let universe = common::line_count();
    let mut rng = Rng::new(4);

    let mut group = c.benchmark_group("filters/static");
    group.sample_size(20);

    for density in [0.01, 0.1, 0.5] {
        let pool: Vec<LineSet> = (0..8)
            .map(|_| LineSet::from(common::sorted_lines(&mut rng, universe, density)))
            .collect();

        for count in [2, 4, 8] {
            let filters = &pool[..count];
            let total: usize = filters.iter().map(LineSet::len).sum();
            group.throughput(Throughput::Elements(total as u64));

            for &(name, strategy) in STRATEGIES {
                group.bench_with_input(
                    BenchmarkId::new(format!("{name}/density_{density}"), count),
                    filters,
                    |b, filters| b.iter(|| black_box(compose(filters, strategy))),
                );
            }
        }
    }

    group.finish();
}

/// Composition of a dense filter with a sparse one, which is the common case of
/// narrowing a broad filter down with a specific one.
fn bench_compose_skewed(c: &mut Criterion) {
    let universe = common::line_count();
    let mut rng = Rng::new(5);
    let filters = [
        LineSet::from(common::sorted_lines(&mut rng, universe, 0.6)),
        LineSet::from(common::sorted_lines(&mut rng, universe, 0.001)),
    ];
    let total: usize = filters.iter().map(LineSet::len).sum();

    let mut group = c.benchmark_group("filters/skewed");
    group.throughput(Throughput::Elements(total as u64));
    group.sample_size(20);

    for &(name, strategy) in STRATEGIES {
        group.bench_function(name, |b| b.iter(|| black_box(compose(&filters, strategy))));
    }

    group.finish();
}

/// End-to-end: run several regex searches and compose them as they stream in,
/// mirroring what happens when a user enables multiple filters in the pager.
fn bench_compose_live_search(c: &mut Criterion) {
    let buf = common::loaded_buffer(SegBuffer::SEG_COUNT);
    let regexes: Vec<Regex> = ["ERROR", "WARN", r"\[db::", "timeout|failed"]
        .into_iter()
        .map(|p| Regex::new(p).unwrap())
        .collect();

    let mut group = c.benchmark_group("filters/live_search");
    group.throughput(Throughput::Bytes(common::log_bytes().len() as u64));
    group.sample_size(10);

    for &(name, strategy) in STRATEGIES {
        group.bench_function(name, |b| {
            b.iter(|| {
                let filters = regexes
                    .iter()
                    .map(|r| LineSet::search(buf.segment_iter().unwrap(), r.clone()))
                    .collect();
                let set = LineSet::compose(filters, true, strategy).unwrap();
                black_box(set.len())
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_compose_static,
    bench_compose_skewed,
    bench_compose_live_search
);
criterion_main!(benches);
