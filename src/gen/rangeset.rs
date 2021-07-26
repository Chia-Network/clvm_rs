use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ops::Bound::{Excluded, Included, Unbounded};

// this is a data structure that holds integer ranges with n log n lookups for
// overlapping ranges.

#[derive(Copy, Clone)]
enum RangeEntry {
    Start(u32),
    End(u32),
}

impl RangeEntry {
    fn index(&self) -> &u32 {
        match self {
            RangeEntry::Start(p) => p,
            RangeEntry::End(p) => p,
        }
    }
}

impl PartialEq for RangeEntry {
    fn eq(&self, other: &Self) -> bool {
        self.index() == other.index()
    }
}

impl Eq for RangeEntry {}

impl Ord for RangeEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.index().cmp(other.index())
    }
}

impl PartialOrd for RangeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.index().cmp(other.index()))
    }
}

pub struct RangeSet {
    entries: BTreeSet<RangeEntry>,
}

impl RangeSet {
    pub fn new() -> Self {
        RangeSet {
            entries: BTreeSet::new(),
        }
    }

    // returns an ordered list of ranges *in between* the ranges that exist in
    // the RangeSet, within the specified [start, end)
    //
    // consider the following RangeSet:
    // ----****------*****----
    // a request for this range:
    //  |--------------|
    // returns:
    //  ***----******--
    pub fn not_overlapping(&self, start: u32, end: u32) -> Vec<(u32, u32)> {
        assert!(start < end);

        let ranges = self.entries.range((
            Included(RangeEntry::Start(start)),
            Included(RangeEntry::End(end)),
        ));
        let mut ret = Vec::<(u32, u32)>::new();

        let mut current_start: Option<u32> = Some(start);
        let mut num = 0;
        for e in ranges {
            num += 1;
            match e {
                RangeEntry::End(p) => {
                    if p < &end {
                        current_start = Some(*p);
                    }
                }
                RangeEntry::Start(p) => {
                    if current_start.unwrap() < *p {
                        ret.push((current_start.unwrap(), *p));
                    }
                    current_start = None;
                }
            }
        }

        if num == 0 {
            // if we didn't find either a start or an end, we don't know whether
            // we're entirely inside a range or entirely outside. Look up the
            // entry before start.
            let mut before = self
                .entries
                .range((Unbounded, Excluded(RangeEntry::Start(start))));
            if let Some(RangeEntry::Start(_)) = before.next_back() {
                // we're entirely inside a range, so return the empty set
                return ret;
            }
        }

        if let Some(s) = current_start {
            if s < end {
                assert!(s < end);
                ret.push((s, end));
            }
        }
        ret
    }

    // adds a new range. This will merge with any existing range
    // end is one-past end
    pub fn add(&mut self, start: u32, end: u32) {
        assert!(start < end);
        let entries = self.entries.range((
            Included(RangeEntry::Start(start)),
            Included(RangeEntry::End(end)),
        ));

        // there is no way of removing a range of entries from a BTreeSet, so we
        // need to copy all the elements out and then remove them one at a time
        let mut to_remove = Vec::<RangeEntry>::new();
        for e in entries {
            to_remove.push(*e);
        }

        if to_remove.is_empty() {
            // if we didn't find either a start or an end, we don't know whether
            // we're entirely inside a range or entirely outside. Look up the
            // entry before start.
            let mut before = self
                .entries
                .range((Unbounded, Excluded(RangeEntry::Start(start))));
            if let Some(RangeEntry::Start(_)) = before.next_back() {
                // if we're entirely inside an existing range, it's a no-op
                return;
            }
        }

        let first: Option<RangeEntry> = to_remove.first().copied();
        let last: Option<RangeEntry> = to_remove.last().copied();

        for e in to_remove {
            self.entries.remove(&e);
        }

        // depending on what kinds of entries we removed, we may or may not need
        // to insert any new ones.

        // e.g. if we removed the end of one range and the start of another, we
        // just merged them and don't need any new entries.

        match first {
            Some(RangeEntry::Start(_)) | None => {
                self.entries.insert(RangeEntry::Start(start));
            }
            _ => (),
        }
        match last {
            Some(RangeEntry::End(_)) | None => {
                self.entries.insert(RangeEntry::End(end));
            }
            _ => (),
        }
    }
}

#[test]
fn test_rangeset() {
    let mut s = RangeSet::new();

    assert_eq!(s.not_overlapping(0, u32::MAX), vec![(0, u32::MAX)]);

    s.add(1, 5);
    // now we have:
    // -****------------------
    assert_eq!(s.not_overlapping(0, u32::MAX), vec![(0, 1), (5, u32::MAX)]);
    assert_eq!(s.not_overlapping(2, u32::MAX), vec![(5, u32::MAX)]);
    assert_eq!(s.not_overlapping(0, 3), vec![(0, 1)]);
    assert_eq!(s.not_overlapping(5, u32::MAX), vec![(5, u32::MAX)]);
    assert_eq!(s.not_overlapping(0, 1), vec![(0, 1)]);
    assert_eq!(s.not_overlapping(1, 10), vec![(5, 10)]);

    s.add(7, 10);
    // now we have:
    // -****--***-------------
    assert_eq!(
        s.not_overlapping(0, u32::MAX),
        vec![(0, 1), (5, 7), (10, u32::MAX)]
    );
    assert_eq!(s.not_overlapping(2, u32::MAX), vec![(5, 7), (10, u32::MAX)]);
    assert_eq!(s.not_overlapping(0, 3), vec![(0, 1)]);
    assert_eq!(s.not_overlapping(0, 1), vec![(0, 1)]);
    assert_eq!(s.not_overlapping(6, u32::MAX), vec![(6, 7), (10, u32::MAX)]);
    assert_eq!(s.not_overlapping(8, u32::MAX), vec![(10, u32::MAX)]);
    assert_eq!(s.not_overlapping(6, 9), vec![(6, 7)]);
    assert_eq!(s.not_overlapping(10, u32::MAX), vec![(10, u32::MAX)]);
    assert_eq!(s.not_overlapping(7, 20), vec![(10, 20)]);

    // join two ranges
    s.add(4, 7);
    // now we have:
    // -*********-------------
    assert_eq!(s.not_overlapping(0, 1), vec![(0, 1)]);
    assert_eq!(s.not_overlapping(0, u32::MAX), vec![(0, 1), (10, u32::MAX)]);
    assert_eq!(s.not_overlapping(3, u32::MAX), vec![(10, u32::MAX)]);
    assert_eq!(s.not_overlapping(0, 7), vec![(0, 1)]);
    assert_eq!(s.not_overlapping(10, u32::MAX), vec![(10, u32::MAX)]);
    assert_eq!(s.not_overlapping(1, 20), vec![(10, 20)]);

    // add entirely within the range
    s.add(4, 7);
    // we still have:
    // -*********-------------
    assert_eq!(s.not_overlapping(0, 1), vec![(0, 1)]);
    assert_eq!(s.not_overlapping(0, u32::MAX), vec![(0, 1), (10, u32::MAX)]);
    assert_eq!(s.not_overlapping(3, u32::MAX), vec![(10, u32::MAX)]);
    assert_eq!(s.not_overlapping(0, 7), vec![(0, 1)]);
    assert_eq!(s.not_overlapping(10, u32::MAX), vec![(10, u32::MAX)]);
    assert_eq!(s.not_overlapping(1, 20), vec![(10, 20)]);
}
