use bvr_core::LineIndex;

fn main() {
    let file = std::fs::File::open("./tests/test_5000000.log").unwrap();

    let start = std::time::Instant::now();
    let index = LineIndex::read_file(file, 1 << 20).unwrap();
    index.wait_complete();
    dbg!(index.line_count());

    let elapsed = start.elapsed();
    println!("{}s", elapsed.as_secs_f64());
}
