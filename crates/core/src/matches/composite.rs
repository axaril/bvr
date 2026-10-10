use crate::{
    cowvec::{CowVecSnapshot, CowVecWriter},
    LineSet, Result,
};

/// One input of a composite, read through snapshots so that merging works on
/// plain slices instead of going through atomics for every element.
struct Input {
    lines: LineSet,
    snapshot: CowVecSnapshot<usize>,
    complete: bool,
    /// Index of the first line in `snapshot` that has not been merged yet.
    pos: usize,
}

impl Input {
    fn new(lines: LineSet) -> Self {
        // Checked before taking the snapshot: if the input is complete, the
        // snapshot is guaranteed to contain all of its lines.
        let complete = lines.is_complete();
        Self {
            snapshot: Self::snapshot(&lines),
            lines,
            complete,
            pos: 0,
        }
    }

    fn snapshot(lines: &LineSet) -> CowVecSnapshot<usize> {
        lines
            .snapshot()
            .expect("`All` inputs are resolved by `LineSet::compose`")
    }

    fn refresh(&mut self) {
        // See `Input::new` for why `complete` is read first.
        self.complete = self.lines.is_complete();
        self.snapshot = Self::snapshot(&self.lines);
    }

    /// Unmerged lines that are at most `bound`.
    fn settled(&self, bound: usize) -> &[usize] {
        let remaining = &self.snapshot[self.pos..];
        &remaining[..remaining.partition_point(|&line| line <= bound)]
    }

    fn is_exhausted(&self) -> bool {
        self.complete && self.pos == self.snapshot.len()
    }
}

// Maximum number of lines merged per round before they are published, which
// keeps the intermediate buffer small and results flowing to readers.
cfg_select! {
    test => {
        const ROUND_LIMIT: usize = 7;
    }
    _ => {
        const ROUND_LIMIT: usize = 1 << 16;
    }
}


#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
pub enum CompositeStrategy {
    Intersection,
    Union,
}

pub(crate) struct LineCompositeWriter {
    pub buf: CowVecWriter<usize>,
    pub strategy: CompositeStrategy,
}

impl LineCompositeWriter {
    pub fn compose(mut self, filters: Vec<LineSet>) -> Result<()> {
        let len = filters.iter().map(|filter| filter.len()).sum::<usize>();
        // Divide by 2 because there may be a lot of overlap, but hopefully this
        // nonscientific guess is good enough
        // In the common case, we only have 1 reallocation so its not too bad
        self.buf.reserve(len / 2);

        let mut inputs = filters.into_iter().map(Input::new).collect::<Vec<_>>();
        let mut merged = Vec::new();

        loop {
            if !self.buf.has_readers() {
                break;
            }

            // Lines up to `bound` are final in every input: complete inputs do
            // not change, and incomplete inputs only append lines greater than
            // the last line they have. `None` means no line is final yet.
            // `blocker` is the incomplete input that limits the bound.
            let mut bound = Some(usize::MAX);
            let mut blocker = None;
            for (i, input) in inputs.iter().enumerate() {
                if !input.complete {
                    let last = input.snapshot.last().copied();
                    if blocker.is_none() || last < bound {
                        bound = last;
                        blocker = Some(i);
                    }
                }
            }

            // Whether the merge stopped early because it produced `ROUND_LIMIT` lines.
            let mut limited = false;

            if let Some(bound) = bound {
                let mut settled = inputs
                    .iter()
                    .map(|input| input.settled(bound))
                    .collect::<Vec<_>>();
                let lens = settled.iter().map(|lines| lines.len()).collect::<Vec<_>>();

                limited = match self.strategy {
                    CompositeStrategy::Union => Self::union(&mut settled, &mut merged, ROUND_LIMIT),
                    CompositeStrategy::Intersection => {
                        Self::intersection(&mut settled, &mut merged, ROUND_LIMIT)
                    }
                };

                let remaining = settled.iter().map(|lines| lines.len()).collect::<Vec<_>>();
                for ((input, len), remaining) in inputs.iter_mut().zip(lens).zip(remaining) {
                    input.pos += len - remaining;
                }

                if let (Some(&last), Some(&first)) = (self.buf.last(), merged.first()) {
                    debug_assert!(first > last);
                }
                self.buf.extend_from_slice(&merged);
                merged.clear();
            }

            let done = match self.strategy {
                CompositeStrategy::Union => inputs.iter().all(Input::is_exhausted),
                CompositeStrategy::Intersection => inputs.iter().any(Input::is_exhausted),
            };
            if done {
                break;
            }

            // Nothing more can be merged until the blocking input grows.
            if let (false, Some(i)) = (limited, blocker) {
                let input = &inputs[i];
                input.lines.wait_for_index(input.snapshot.len());
            }

            for input in inputs.iter_mut().filter(|input| !input.complete) {
                input.refresh();
            }
        }
        Ok(())
    }

    /// Appends the union of the sorted `lists` to `out`, advancing past the
    /// merged lines. Returns `true` if it stopped early after `limit` lines.
    fn union(lists: &mut [&[usize]], out: &mut Vec<usize>, limit: usize) -> bool {
        while let Some(min) = lists
            .iter()
            .filter_map(|lines| lines.first())
            .min()
            .copied()
        {
            if out.len() >= limit {
                return true;
            }
            out.push(min);
            for lines in lists.iter_mut() {
                if lines.first() == Some(&min) {
                    *lines = &lines[1..];
                }
            }
        }
        false
    }

    /// Appends the intersection of the sorted `lists` to `out`, advancing past the
    /// lines it has examined. Returns `true` if it stopped early after `limit` lines.
    fn intersection(lists: &mut [&[usize]], out: &mut Vec<usize>, limit: usize) -> bool {
        loop {
            if out.len() >= limit {
                return true;
            }

            // The smallest line that could be in every list.
            let mut candidate = 0;
            for lines in lists.iter() {
                match lines.first() {
                    Some(&line) => candidate = candidate.max(line),
                    None => return false,
                }
            }

            let mut in_all = true;
            for lines in lists.iter_mut() {
                *lines = &lines[Self::gallop(lines, candidate)..];
                match lines.first() {
                    Some(&line) => in_all &= line == candidate,
                    None => return false,
                }
            }

            if in_all {
                out.push(candidate);
                for lines in lists.iter_mut() {
                    *lines = &lines[1..];
                }
            }
        }
    }

    /// Returns the index of the first line in sorted `lines` that is not less
    /// than `target`, searching forward from the start.
    fn gallop(lines: &[usize], target: usize) -> usize {
        let mut lo = 0;
        let mut step = 1;
        // Invariant: lines[lo] < target, except when lo == 0.
        while lo + step < lines.len() && lines[lo + step] < target {
            lo += step;
            step *= 2;
        }
        let hi = (lo + step + 1).min(lines.len());
        lo + lines[lo..hi].partition_point(|&line| line < target)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::CompositeStrategy;
    use crate::{cowvec::CowVec, LineSet};

    /// Inputs are written by other threads while the composite is computed,
    /// in uneven batches, and the result must match a reference computed on
    /// complete sets.
    #[test]
    fn test_composite_with_incomplete_inputs() {
        let mut rng: u64 = 0x2545F4914F6CDD1D;
        let mut next = move |n: u64| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng % n
        };

        for round in 0..60 {
            let input_count = 2 + next(4) as usize;
            let density = 1 + next(8);
            let sets = (0..input_count)
                .map(|_| {
                    (0..2000)
                        .filter(|_| next(10) < density)
                        .collect::<Vec<usize>>()
                })
                .collect::<Vec<_>>();

            for strategy in [CompositeStrategy::Union, CompositeStrategy::Intersection] {
                let expected = match strategy {
                    CompositeStrategy::Union => sets.iter().flatten().copied().collect(),
                    CompositeStrategy::Intersection => sets[0]
                        .iter()
                        .copied()
                        .filter(|line| sets.iter().all(|set| set.binary_search(line).is_ok()))
                        .collect::<BTreeSet<_>>(),
                }
                .into_iter()
                .collect::<Vec<_>>();

                let mut writers = Vec::new();
                let inputs = sets
                    .iter()
                    .map(|set| {
                        let (buf, writer) = CowVec::new();
                        let batch = 1 + next(300) as usize;
                        writers.push((writer, set.clone(), batch));
                        LineSet::Dynamic { buf, min_len: 0 }
                    })
                    .collect::<Vec<_>>();

                let composite = LineSet::compose(inputs, false, strategy).unwrap();

                let handles = writers
                    .into_iter()
                    .map(|(mut writer, set, batch)| {
                        std::thread::spawn(move || {
                            for chunk in set.chunks(batch) {
                                writer.extend_from_slice(chunk);
                                std::thread::yield_now();
                            }
                        })
                    })
                    .collect::<Vec<_>>();
                for handle in handles {
                    handle.join().unwrap();
                }

                composite.wait_for_index(usize::MAX);
                assert!(composite.is_complete());
                let LineSet::Dynamic { buf, .. } = &composite else {
                    unreachable!()
                };
                assert_eq!(
                    buf.snapshot().as_slice(),
                    expected.as_slice(),
                    "round {round}, {strategy:?}"
                );
            }
        }
    }

    #[test]
    fn test_composite_with_all() {
        let (index, _writer) = crate::LineIndex::new(crate::index::ProgressReport::NONE);
        let all = LineSet::All { buf: index };
        let some = LineSet::from(vec![1, 3, 5]);

        let union = LineSet::compose(
            vec![some.clone(), all.clone()],
            true,
            CompositeStrategy::Union,
        )
        .unwrap();
        assert!(union.is_all());

        let intersection = LineSet::compose(
            vec![all.clone(), some.clone(), all.clone()],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();
        assert_eq!(intersection.find(3), Some(1));
        assert_eq!(intersection.len(), 3);

        let only_all = LineSet::compose(
            vec![all.clone(), all],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();
        assert!(only_all.is_all());
    }

    #[test]
    fn test_composite_union_basic() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![1, 3, 5, 7, 9]);

        let composite =
            LineSet::compose(vec![matches1, matches2], true, CompositeStrategy::Union).unwrap();

        let result = vec![1, 2, 3, 4, 5, 7, 9];

        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }

    #[test]
    fn test_composite_union_disjoint() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![6, 7, 8, 9, 10]);

        let composite =
            LineSet::compose(vec![matches1, matches2], true, CompositeStrategy::Union).unwrap();

        let result = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }

    #[test]
    fn test_composite_union_same() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![1, 2, 3, 4, 5]);

        let composite =
            LineSet::compose(vec![matches1, matches2], true, CompositeStrategy::Union).unwrap();

        let result = vec![1, 2, 3, 4, 5];

        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }

    #[test]
    fn test_composite_union_empty() {
        let matches1 = LineSet::from(vec![]);
        let matches2 = LineSet::from(vec![1, 2, 3, 4, 5]);

        let composite =
            LineSet::compose(vec![matches1, matches2], true, CompositeStrategy::Union).unwrap();

        let result = vec![1, 2, 3, 4, 5];

        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }

    #[test]
    fn test_composite_union_empty_all() {
        let matches1 = LineSet::from(vec![]);
        let matches2 = LineSet::from(vec![]);

        let composite =
            LineSet::compose(vec![matches1, matches2], true, CompositeStrategy::Union).unwrap();

        let inner = composite.into_inner();
        assert_eq!(0, inner.len());
    }

    #[test]
    fn test_composite_union_empty_all_but_one() {
        let matches1 = LineSet::from(vec![]);
        let matches2 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches3 = LineSet::from(vec![]);

        let composite = LineSet::compose(
            vec![matches1, matches2, matches3],
            true,
            CompositeStrategy::Union,
        )
        .unwrap();

        let result = vec![1, 2, 3, 4, 5];

        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }

    #[test]
    fn test_composite_union_many() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![1, 3, 5, 7, 9]);
        let matches3 = LineSet::from(vec![2, 4, 6, 8, 10]);
        let matches4 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches5 = LineSet::from(vec![1, 3, 5, 7, 9]);
        let matches6 = LineSet::from(vec![2, 4, 6, 8, 10]);
        let matches7 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches8 = LineSet::from(vec![1, 3, 5, 7, 9]);
        let matches9 = LineSet::from(vec![2, 4, 6, 8, 10]);
        let matches10 = LineSet::from(vec![1, 2, 3, 4, 5]);

        let composite = LineSet::compose(
            vec![
                matches1, matches2, matches3, matches4, matches5, matches6, matches7, matches8,
                matches9, matches10,
            ],
            true,
            CompositeStrategy::Union,
        )
        .unwrap();

        let result = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }

    #[test]
    fn test_composite_union_many_disjoint() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![6, 7, 8, 9, 10]);
        let matches3 = LineSet::from(vec![11, 12, 13, 14, 15]);
        let matches4 = LineSet::from(vec![16, 17, 18, 19, 20]);
        let matches5 = LineSet::from(vec![21, 22, 23, 24, 25]);
        let matches6 = LineSet::from(vec![26, 27, 28, 29, 30]);
        let matches7 = LineSet::from(vec![31, 32, 33, 34, 35]);
        let matches8 = LineSet::from(vec![36, 37, 38, 39, 40]);
        let matches9 = LineSet::from(vec![41, 42, 43, 44, 45]);
        let matches10 = LineSet::from(vec![46, 47, 48, 49, 50]);

        let composite = LineSet::compose(
            vec![
                matches1, matches2, matches3, matches4, matches5, matches6, matches7, matches8,
                matches9, matches10,
            ],
            true,
            CompositeStrategy::Union,
        )
        .unwrap();

        let result = vec![
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, //
            11, 12, 13, 14, 15, 16, 17, 18, 19, 20, //
            21, 22, 23, 24, 25, 26, 27, 28, 29, 30, //
            31, 32, 33, 34, 35, 36, 37, 38, 39, 40, //
            41, 42, 43, 44, 45, 46, 47, 48, 49, 50, //
        ];

        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }

    #[test]
    fn test_composite_intersection_basic() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![1, 3, 5, 7, 9]);

        let composite = LineSet::compose(
            vec![matches1, matches2],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();

        let result = vec![1, 3, 5];

        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }

    #[test]
    fn test_composite_intersection_disjoint() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![6, 7, 8, 9, 10]);

        let composite = LineSet::compose(
            vec![matches1, matches2],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();

        let inner = composite.into_inner();
        assert_eq!(0, inner.len());
    }

    #[test]
    fn test_composite_intersection_same() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![1, 2, 3, 4, 5]);

        let composite = LineSet::compose(
            vec![matches1, matches2],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();

        let result = vec![1, 2, 3, 4, 5];

        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }

    #[test]
    fn test_composite_intersection_empty() {
        let matches1 = LineSet::from(vec![]);
        let matches2 = LineSet::from(vec![1, 2, 3, 4, 5]);

        let composite = LineSet::compose(
            vec![matches1, matches2],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();

        let inner = composite.into_inner();
        assert_eq!(0, inner.len());
    }

    #[test]
    fn test_composite_intersection_empty_all() {
        let matches1 = LineSet::from(vec![]);
        let matches2 = LineSet::from(vec![]);

        let composite = LineSet::compose(
            vec![matches1, matches2],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();

        let inner = composite.into_inner();
        assert_eq!(0, inner.len());
    }

    #[test]
    fn test_composite_intersection_empty_all_but_one() {
        let matches1 = LineSet::from(vec![]);
        let matches2 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches3 = LineSet::from(vec![]);

        let composite = LineSet::compose(
            vec![matches1, matches2, matches3],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();

        let inner = composite.into_inner();
        assert_eq!(0, inner.len());
    }

    #[test]
    fn test_composite_intersection_many() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![1, 3, 5, 7, 9]);
        let matches3 = LineSet::from(vec![2, 4, 6, 8, 10]);
        let matches4 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches5 = LineSet::from(vec![1, 3, 5, 7, 9]);
        let matches6 = LineSet::from(vec![2, 4, 6, 8, 10]);
        let matches7 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches8 = LineSet::from(vec![1, 3, 5, 7, 9]);
        let matches9 = LineSet::from(vec![2, 4, 6, 8, 10]);
        let matches10 = LineSet::from(vec![1, 2, 3, 4, 5]);

        let composite = LineSet::compose(
            vec![
                matches1, matches2, matches3, matches4, matches5, matches6, matches7, matches8,
                matches9, matches10,
            ],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();

        let inner = composite.into_inner();
        assert_eq!(0, inner.len());
    }

    #[test]
    fn test_composite_intersection_many_disjoint() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5]);
        let matches2 = LineSet::from(vec![6, 7, 8, 9, 10]);
        let matches3 = LineSet::from(vec![11, 12, 13, 14, 15]);
        let matches4 = LineSet::from(vec![16, 17, 18, 19, 20]);
        let matches5 = LineSet::from(vec![21, 22, 23, 24, 25]);
        let matches6 = LineSet::from(vec![26, 27, 28, 29, 30]);
        let matches7 = LineSet::from(vec![31, 32, 33, 34, 35]);
        let matches8 = LineSet::from(vec![36, 37, 38, 39, 40]);
        let matches9 = LineSet::from(vec![41, 42, 43, 44, 45]);
        let matches10 = LineSet::from(vec![46, 47, 48, 49, 50]);

        let composite = LineSet::compose(
            vec![
                matches1, matches2, matches3, matches4, matches5, matches6, matches7, matches8,
                matches9, matches10,
            ],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();

        let inner = composite.into_inner();
        assert_eq!(0, inner.len());
    }

    #[test]
    fn test_composite_intersection_many_some() {
        let matches1 = LineSet::from(vec![1, 2, 3, 4, 5, 10, 20, 30, 40, 50, 51, 213]);
        let matches2 = LineSet::from(vec![6, 7, 8, 9, 10, 15, 20, 30, 40, 45, 50, 51]);
        let matches3 = LineSet::from(vec![10, 11, 12, 13, 14, 15, 20, 30, 35, 40, 50, 60]);
        let matches4 = LineSet::from(vec![10, 16, 17, 18, 19, 20, 30, 40, 50, 51, 52, 53]);
        let matches5 = LineSet::from(vec![10, 20, 21, 22, 23, 24, 25, 30, 32, 33, 35, 40, 50]);
        let matches6 = LineSet::from(vec![2, 10, 20, 26, 27, 28, 29, 30, 40, 50, 51, 52, 53]);
        let matches7 = LineSet::from(vec![10, 20, 30, 31, 32, 33, 34, 35, 40, 50, 51, 52, 53]);
        let matches8 = LineSet::from(vec![10, 20, 25, 30, 36, 37, 38, 39, 40, 45, 50]);
        let matches9 = LineSet::from(vec![4, 10, 20, 30, 40, 41, 42, 43, 44, 45, 50, 51, 52, 53]);
        let matches10 = LineSet::from(vec![1, 10, 20, 30, 40, 46, 47, 48, 49, 50]);

        let composite = LineSet::compose(
            vec![
                matches1, matches2, matches3, matches4, matches5, matches6, matches7, matches8,
                matches9, matches10,
            ],
            true,
            CompositeStrategy::Intersection,
        )
        .unwrap();

        let result = vec![10, 20, 30, 40, 50];
        let inner = composite.into_inner();
        for i in 0..result.len() {
            assert_eq!(result.get(i).copied(), inner.get(i));
        }
    }
}
