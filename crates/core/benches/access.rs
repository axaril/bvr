//! Benchmarks for random and sequential access into an already built index,
//! buffer, and line sets.

mod common;

use std::{hint::black_box, num::NonZeroUsize};

use bvr_core::{LineSet, SegBuffer};
use common::Rng;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};

const BATCH: usize = 4096;

fn bench_index_lookup(c: &mut Criterion) {
    let buf = common::loaded_buffer(SegBuffer::SEG_COUNT);
    let index = buf.index();
    let lines = index.line_count();
    let data_len = common::log_bytes().len() as u64;

    let mut rng = Rng::new(1);
    let random = common::random_lines(&mut rng, lines, BATCH);
    let offsets: Vec<u64> = (0..BATCH).map(|_| rng.below(data_len)).collect();

    let mut group = c.benchmark_group("access/index");
    group.throughput(Throughput::Elements(BATCH as u64));

    group.bench_function("data_of_line/sequential", |b| {
        let mut start = 0;
        b.iter(|| {
            for line in start..start + BATCH {
                black_box(index.data_of_line(line));
            }
            start = (start + BATCH) % (lines - BATCH);
        })
    });

    group.bench_function("data_of_line/random", |b| {
        b.iter(|| {
            for &line in &random {
                black_box(index.data_of_line(line));
            }
        })
    });

    group.bench_function("line_of_data/random", |b| {
        b.iter(|| {
            for &offset in &offsets {
                black_box(index.line_of_data(offset));
            }
        })
    });

    group.finish();
}

fn bench_get_line(c: &mut Criterion) {
    let segments_in_file = common::log_bytes().len().div_ceil(1 << 20) + 1;

    let mut group = c.benchmark_group("access/get_line");
    group.throughput(Throughput::Elements(BATCH as u64));

    // Default segment cache. Sequential reads should almost always hit the cache.
    {
        let buf = common::loaded_buffer(SegBuffer::SEG_COUNT);
        let lines = buf.line_count();
        let random = common::random_lines(&mut Rng::new(2), lines, BATCH);

        group.bench_function("sequential", |b| {
            let mut start = 0;
            b.iter(|| {
                for line in start..start + BATCH {
                    black_box(buf.get_line(line));
                }
                start = (start + BATCH) % (lines - BATCH);
            })
        });

        group.bench_function("random/default_cache", |b| {
            b.iter(|| {
                for &line in &random {
                    black_box(buf.get_line(line));
                }
            })
        });
    }

    // Tiny segment cache: random access thrashes the LRU and remaps segments.
    {
        let buf = common::loaded_buffer(NonZeroUsize::new(2).unwrap());
        let random = common::random_lines(&mut Rng::new(2), buf.line_count(), BATCH);
        group.bench_function("random/cold_cache", |b| {
            b.iter(|| {
                for &line in &random {
                    black_box(buf.get_line(line));
                }
            })
        });
    }

    // Cache large enough to hold every segment: random access never remaps.
    {
        let buf = common::loaded_buffer(NonZeroUsize::new(segments_in_file).unwrap());
        let random = common::random_lines(&mut Rng::new(2), buf.line_count(), BATCH);
        for &line in &random {
            buf.get_line(line);
        }
        group.bench_function("random/warm_cache", |b| {
            b.iter(|| {
                for &line in &random {
                    black_box(buf.get_line(line));
                }
            })
        });
    }

    group.finish();
}

fn bench_lineset_lookup(c: &mut Criterion) {
    let buf = common::loaded_buffer(SegBuffer::SEG_COUNT);
    let lines = buf.line_count();

    let mut rng = Rng::new(3);
    let dynamic = LineSet::from(common::sorted_lines(&mut rng, lines, 0.1));
    let all = buf.all_line_matches();
    let probes = common::random_lines(&mut rng, lines, BATCH);
    let positions = common::random_lines(&mut rng, dynamic.len(), BATCH);

    let mut group = c.benchmark_group("access/lineset");
    group.throughput(Throughput::Elements(BATCH as u64));

    for (name, set, pos) in [("all", &all, &probes), ("dynamic", &dynamic, &positions)] {
        group.bench_function(format!("{name}/get"), |b| {
            b.iter(|| {
                for &i in pos {
                    black_box(set.get(i));
                }
            })
        });
        group.bench_function(format!("{name}/find"), |b| {
            b.iter(|| {
                for &line in &probes {
                    black_box(set.find(line));
                }
            })
        });
        group.bench_function(format!("{name}/nearest_forward"), |b| {
            b.iter(|| {
                for &line in &probes {
                    black_box(set.nearest_forward(line));
                }
            })
        });
        group.bench_function(format!("{name}/nearest_backward"), |b| {
            b.iter(|| {
                for &line in &probes {
                    black_box(set.nearest_backward(line));
                }
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_index_lookup,
    bench_get_line,
    bench_lineset_lookup
);
criterion_main!(benches);
