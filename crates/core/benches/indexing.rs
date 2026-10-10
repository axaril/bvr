//! Benchmarks for building the line index from files and streams.

mod common;

use std::hint::black_box;

use bvr_core::{index::IndexType, LineIndex, SegBuffer};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

const SEGMENT_SIZE: u64 = 1 << 20;

fn index_file<I: IndexType>(segment_size: u64) -> usize {
    let index = LineIndex::<I>::read_file(common::open_log(), segment_size).unwrap();
    index.wait_complete();
    index.line_count()
}

fn bench_index_file(c: &mut Criterion) {
    let len = common::log_bytes().len() as u64;
    let mut group = c.benchmark_group("index/file");
    group.throughput(Throughput::Bytes(len));
    group.sample_size(20);

    for segment_size in [1 << 16, 1 << 18, 1 << 20, 1 << 22] {
        group.bench_with_input(
            BenchmarkId::new("u32/segment_size", segment_size),
            &segment_size,
            |b, &segment_size| b.iter(|| black_box(index_file::<u32>(segment_size))),
        );
    }

    group.bench_function("u16/segment_size/1048576", |b| {
        b.iter(|| black_box(index_file::<u16>(SEGMENT_SIZE)))
    });

    group.finish();
}

fn bench_index_stream(c: &mut Criterion) {
    let bytes = common::log_bytes();
    let mut group = c.benchmark_group("index/stream");
    group.throughput(Throughput::Bytes(bytes.len() as u64));
    group.sample_size(20);

    group.bench_function("in_memory", |b| {
        b.iter(|| {
            let (index, rx) = LineIndex::<u32>::read_stream(Box::new(bytes), SEGMENT_SIZE).unwrap();
            index.wait_complete();
            // Keep the segment receiver alive until indexing finishes, otherwise
            // the writer bails out early when it fails to hand off segments.
            drop(rx);
            black_box(index.line_count())
        })
    });

    group.bench_function("file_backed", |b| {
        b.iter(|| {
            let file = tempfile::tempfile().unwrap();
            let (index, _len) =
                LineIndex::<u32>::read_stream_file_backed(Box::new(bytes), file, SEGMENT_SIZE)
                    .unwrap();
            index.wait_complete();
            black_box(index.line_count())
        })
    });

    group.finish();
}

fn bench_segbuffer_open(c: &mut Criterion) {
    let len = common::log_bytes().len() as u64;
    let mut group = c.benchmark_group("index/segbuffer");
    group.throughput(Throughput::Bytes(len));
    group.sample_size(20);

    group.bench_function("read_file", |b| {
        b.iter(|| {
            let buf = SegBuffer::read_file(common::open_log(), SegBuffer::SEG_COUNT).unwrap();
            buf.wait_complete();
            black_box(buf.line_count())
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_index_file,
    bench_index_stream,
    bench_segbuffer_open
);
criterion_main!(benches);
