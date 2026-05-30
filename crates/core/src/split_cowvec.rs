use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::cowvec::{CowVec, CowVecSnapshot, CowVecWriter};

/// An exclusive writer to a `SplitCowVec<T>`.
///
/// This writer manages multiple `CowVec<T>` segments, creating a new segment
/// every `elements_per_segment` elements. The writer and reader can coexist,
/// allowing concurrent reads while writing.
pub struct SplitCowVecWriter<T> {
    elements_per_segment: usize,
    target: Arc<SplitCowVec<T>>,
    current: Option<CowVecWriter<T>>,
}

impl<T> SplitCowVecWriter<T>
where
    T: Copy,
{
    /// Appends an element to the back of the split vector.
    ///
    /// This operation is O(1) amortized. When the current segment reaches
    /// `elements_per_segment`, a new segment is created.
    pub fn push(&mut self, elem: T) {
        let writer = if let Some(writer) = self.current.as_mut() {
            if writer.len() >= self.elements_per_segment {
                self.new_segment()
            } else {
                writer
            }
        } else {
            self.new_segment()
        };

        writer.push(elem);
    }
}

impl<T> SplitCowVecWriter<T> {
    fn new_segment(&mut self) -> &mut CowVecWriter<T> {
        let (new_segment, writer) = CowVec::new();

        let segments = self
            .target
            .segments
            .load()
            .iter()
            .cloned()
            .chain(Some(new_segment.clone()))
            .collect::<Box<[_]>>();

        self.target.segments.store(Arc::new(segments));
        self.current.insert(writer)
    }

    /// Returns the total number of elements written so far.
    pub fn len(&self) -> usize {
        self.target
            .segments
            .load()
            .split_last()
            .map(|(last, prev)| prev.len() * self.elements_per_segment + last.len())
            .unwrap_or(0)
    }

    /// Returns true if no elements have been written.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of segments (completed + current).
    pub fn segment_count(&self) -> usize {
        self.target.segments.load().len()
    }
}

/// A read-only view of a split copy-on-write vector.
///
/// This is composed of multiple `CowVec<T>` segments, each containing up to
/// `elements_per_segment` elements. This can coexist with a `SplitCowVecWriter`,
/// allowing concurrent reads while writing.
pub struct SplitCowVec<T> {
    elements_per_segment: usize,
    segments: ArcSwap<Box<[Arc<CowVec<T>>]>>,
}

impl<T> SplitCowVec<T> {
    /// Constructs a new, empty `SplitCowVec<T>` with a write handle.
    ///
    /// The vector will not allocate until elements are pushed onto it.
    ///
    /// # Arguments
    /// * `elements_per_segment` - Number of elements per segment before creating a new CowVec
    pub fn new(elements_per_segment: usize) -> (Arc<Self>, SplitCowVecWriter<T>) {
        let initial_segments: Arc<Box<[_]>> = Arc::new(Box::new([]));

        let cow = Arc::new(Self {
            elements_per_segment,
            segments: ArcSwap::new(initial_segments.clone()),
        });

        let writer = SplitCowVecWriter {
            elements_per_segment,
            target: cow.clone(),
            current: None,
        };

        (cow, writer)
    }

    /// Constructs a new, empty `SplitCowVec<T>` with default configuration (1024 elements per segment).
    pub fn with_default_config() -> (Arc<Self>, SplitCowVecWriter<T>) {
        Self::new(1024)
    }

    /// Returns the total number of elements across all segments.
    pub fn len(&self) -> usize {
        self.segments
            .load()
            .split_last()
            .map(|(last, prev)| prev.len() * self.elements_per_segment + last.len())
            .unwrap_or(0)
    }

    /// Returns the number of segments.
    pub fn segment_count(&self) -> usize {
        self.segments.load().len()
    }

    /// Returns true if the split vector contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Takes an atomic snapshot of all segments at the current point in time.
    ///
    /// This snapshot pins all internal buffers, ensuring a consistent view
    /// across all segments even if writes occur after the snapshot is taken.
    pub fn snapshot(&self) -> SplitCowVecSnapshot<T> {
        let snapshots = self
            .segments
            .load()
            .iter()
            .map(|seg| seg.snapshot())
            .collect();

        SplitCowVecSnapshot { snapshots }
    }
}

impl<T> SplitCowVec<T>
where
    T: Copy,
{
    /// Returns the element at the given index, or `None` if out of bounds.
    pub fn get(&self, index: usize) -> Option<T> {
        let eps = self.elements_per_segment;
        let segments = self.segments.load();
        let seg = segments.get(index / eps)?;
        seg.get(index % eps)
    }

    pub unsafe fn get_unchecked(&self, index: usize) -> T {
        self.get(index).unwrap_unchecked()
    }
}

impl<T> std::fmt::Debug for SplitCowVec<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SplitCowVec[..]")
    }
}

/// A snapshot of a `SplitCowVec<T>` at a point in time.
pub struct SplitCowVecSnapshot<T> {
    snapshots: Vec<CowVecSnapshot<T>>,
}

impl<T> SplitCowVecSnapshot<T> {
    /// Returns the number of segments in this snapshot.
    pub fn segment_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns a snapshot of the segment at the given index, or `None` if out of bounds.
    pub fn get_segment(&self, index: usize) -> Option<&CowVecSnapshot<T>> {
        self.snapshots.get(index)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Barrier,
        },
        thread,
    };

    use super::*;

    #[test]
    fn test_concurrent_reads_while_writing() {
        const READER_THREADS: usize = 8;
        const TOTAL: usize = 10_000;
        const SEG_SIZE: usize = 256;

        let (vec, mut writer) = SplitCowVec::<usize>::new(SEG_SIZE);
        let barrier = Arc::new(Barrier::new(READER_THREADS + 1));
        let done = Arc::new(AtomicBool::new(false));

        let handles: Vec<_> = (0..READER_THREADS)
            .map(|_| {
                let vec = vec.clone();
                let barrier = barrier.clone();
                let done = done.clone();
                thread::spawn(move || {
                    barrier.wait();
                    while !done.load(Ordering::Acquire) {
                        // Only iterate up to the observed length to avoid spurious None returns.
                        let len = vec.len();
                        for i in 0..len {
                            if let Some(val) = vec.get(i) {
                                assert_eq!(val, i, "data corruption at index {i}");
                            }
                        }
                    }
                    // After writer drops every element must be readable.
                    for i in 0..TOTAL {
                        assert_eq!(vec.get(i), Some(i), "missing element at index {i}");
                    }
                })
            })
            .collect();

        barrier.wait();
        for i in 0..TOTAL {
            writer.push(i);
        }
        drop(writer);
        done.store(true, Ordering::Release);

        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// All segments except the last must have exactly `elements_per_segment` elements.
    /// This invariant must hold for every snapshot taken concurrently with writes.
    #[test]
    fn test_snapshot_segment_size_invariant() {
        const THREADS: usize = 4;
        const TOTAL: usize = 5_000;
        const SEG_SIZE: usize = 64;

        let (vec, mut writer) = SplitCowVec::<usize>::new(SEG_SIZE);
        let barrier = Arc::new(Barrier::new(THREADS + 1));
        let done = Arc::new(AtomicBool::new(false));

        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                let vec = vec.clone();
                let barrier = barrier.clone();
                let done = done.clone();
                thread::spawn(move || {
                    barrier.wait();
                    while !done.load(Ordering::Acquire) {
                        let snap = vec.snapshot();
                        let n = snap.segment_count();
                        // Segments 0..n-1 must be exactly full.
                        for i in 0..n.saturating_sub(1) {
                            let seg = snap.get_segment(i).unwrap();
                            assert_eq!(
                                seg.len(),
                                SEG_SIZE,
                                "segment {i}/{n} has {} elements, expected {SEG_SIZE}",
                                seg.len()
                            );
                        }
                        // Last segment may have 0..=SEG_SIZE elements.
                        if n > 0 {
                            let last = snap.get_segment(n - 1).unwrap();
                            assert!(last.len() <= SEG_SIZE);
                        }
                    }
                })
            })
            .collect();

        barrier.wait();
        for i in 0..TOTAL {
            writer.push(i);
        }
        drop(writer);
        done.store(true, Ordering::Release);

        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// Multiple reader threads hammering different index ranges, covering all segment
    /// boundaries, while the writer is active.
    #[test]
    fn test_concurrent_readers_across_segments() {
        const TOTAL: usize = 3_000;
        const SEG_SIZE: usize = 7; // prime to hit non-power-of-two boundaries
        const READER_THREADS: usize = 6;

        let (vec, mut writer) = SplitCowVec::<usize>::new(SEG_SIZE);

        for i in 0..TOTAL {
            writer.push(i);
        }
        drop(writer);

        // All data written before spawning threads — focus on read correctness.
        let barrier = Arc::new(Barrier::new(READER_THREADS));
        let handles: Vec<_> = (0..READER_THREADS)
            .map(|thread_id| {
                let vec = vec.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    // Stride-based access to interleave threads across segment boundaries.
                    let mut i = thread_id;
                    while i < TOTAL {
                        assert_eq!(
                            vec.get(i),
                            Some(i),
                            "thread {thread_id}: wrong value at {i}"
                        );
                        i += READER_THREADS;
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// After the writer is dropped every snapshot should see a consistent, complete
    /// picture: correct per-segment element counts and correct values.
    #[test]
    fn test_snapshot_consistency_after_completion() {
        const TOTAL: usize = 500;
        const SEG_SIZE: usize = 32;

        let (vec, mut writer) = SplitCowVec::<usize>::new(SEG_SIZE);
        for i in 0..TOTAL {
            writer.push(i);
        }
        drop(writer);

        let expected_segments = TOTAL.div_ceil(SEG_SIZE);
        let snap = vec.snapshot();
        assert_eq!(snap.segment_count(), expected_segments);

        // All segments except the last are full.
        for i in 0..expected_segments - 1 {
            assert_eq!(snap.get_segment(i).unwrap().len(), SEG_SIZE);
        }
        // Last segment holds the remainder.
        let remainder = TOTAL % SEG_SIZE;
        let last_len = if remainder == 0 { SEG_SIZE } else { remainder };
        assert_eq!(
            snap.get_segment(expected_segments - 1).unwrap().len(),
            last_len
        );

        // Values are correct across all segments.
        let mut global_idx = 0;
        for seg_idx in 0..expected_segments {
            let seg = snap.get_segment(seg_idx).unwrap();
            for &val in seg.iter() {
                assert_eq!(val, global_idx, "wrong value at global index {global_idx}");
                global_idx += 1;
            }
        }
    }

    /// High-contention stress test: many threads reading while the writer produces
    /// many small segments.
    #[test]
    fn test_high_contention_small_segments() {
        const READER_THREADS: usize = 16;
        const TOTAL: usize = 8_000;
        const SEG_SIZE: usize = 8;

        let (vec, mut writer) = SplitCowVec::<usize>::new(SEG_SIZE);
        let barrier = Arc::new(Barrier::new(READER_THREADS + 1));
        let done = Arc::new(AtomicBool::new(false));

        let handles: Vec<_> = (0..READER_THREADS)
            .map(|_| {
                let vec = vec.clone();
                let barrier = barrier.clone();
                let done = done.clone();
                thread::spawn(move || {
                    barrier.wait();
                    while !done.load(Ordering::Acquire) {
                        let len = vec.len();
                        for i in 0..len {
                            if let Some(val) = vec.get(i) {
                                assert_eq!(val, i);
                            }
                        }
                        std::hint::spin_loop();
                    }
                    for i in 0..TOTAL {
                        assert_eq!(vec.get(i), Some(i));
                    }
                })
            })
            .collect();

        barrier.wait();
        for i in 0..TOTAL {
            writer.push(i);
            if i % 100 == 0 {
                thread::yield_now();
            }
        }
        drop(writer);
        done.store(true, Ordering::Release);

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_split_cowvec_basic() {
        let (vec, mut writer) = SplitCowVec::new(5);

        for i in 0..12 {
            writer.push(i);
        }

        drop(writer);

        assert_eq!(vec.len(), 12);
        assert_eq!(vec.segment_count(), 3); // 5 + 5 + 2
    }

    #[test]
    fn test_split_cowvec_single_segment() {
        let (vec, mut writer) = SplitCowVec::new(100);

        for i in 0..10 {
            writer.push(i);
        }

        drop(writer);

        assert_eq!(vec.len(), 10);
        assert_eq!(vec.segment_count(), 1);
    }

    #[test]
    fn test_split_cowvec_empty() {
        let (_vec, _writer) = SplitCowVec::<i32>::with_default_config();
        // Writer is dropped, so segments are finalized
        assert!(_vec.is_empty());
        assert_eq!(_vec.segment_count(), 0);
    }

    #[test]
    fn test_split_cowvec_snapshot() {
        let (vec, mut writer) = SplitCowVec::new(3);

        for i in 0..7 {
            writer.push(i);
        }

        drop(writer);

        let snapshot = vec.snapshot();
        assert_eq!(snapshot.segment_count(), 3);
    }
}
