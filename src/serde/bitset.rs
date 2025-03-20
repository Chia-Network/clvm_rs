/// This is a simple bitfield used to indicates whether a node has been visited
/// during a tree search or not. We terminate a search path if we've reached the
/// node first via a different (shorter) path.
#[derive(Clone, Default)]
pub struct BitSet {
    bits: Vec<usize>,
}

impl BitSet {
    const BITS: usize = usize::BITS as usize;

    /// specify the number of nodes to track
    pub fn new(max_idx: u32) -> Self {
        let bits = vec![0; (max_idx as usize + Self::BITS) / Self::BITS];
        Self { bits }
    }

    /// marks the specified node as visited and returns whether it had already
    /// been marked.
    pub fn visit(&mut self, idx: u32) -> bool {
        let pos = idx as usize / Self::BITS;
        let mask = (1_usize) << (idx as usize % Self::BITS);
        let ret = self.bits[pos] & mask;
        self.bits[pos] |= mask;
        ret != 0
    }

    pub fn is_visited(&self, idx: u32) -> bool {
        let pos = idx as usize / Self::BITS;
        let mask = (1_usize) << (idx as usize % Self::BITS);
        (self.bits[pos] & mask) != 0
    }

    pub fn extend(&mut self, max_idx: u32) {
        let new_len = (max_idx as usize + Self::BITS) / Self::BITS;
        assert!(max_idx as usize >= self.bits.len());
        self.bits.resize(new_len, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visited_nodes() {
        let mut n = BitSet::new(100);
        for i in 0..100 {
            assert!(!n.is_visited(i));
            assert!(!n.visit(i));
            assert!(n.is_visited(i));
            assert!(n.visit(i));
            assert!(n.is_visited(i));
        }
    }

    #[test]
    fn test_visited_nodes_reverse() {
        let mut n = BitSet::new(100);
        for i in (0..100).rev() {
            assert!(!n.is_visited(i));
            assert!(!n.visit(i));
            assert!(n.is_visited(i));
            assert!(n.visit(i));
            assert!(n.is_visited(i));
        }
    }

    #[test]
    fn test_extend() {
        let mut n = BitSet::default();
        n.extend(1);
        assert!(!n.is_visited(0));
        assert!(!n.visit(0));
        assert!(n.is_visited(0));

        n.extend(2);
        assert!(n.is_visited(0));

        assert!(!n.is_visited(1));
        assert!(!n.visit(1));
        assert!(n.is_visited(1));

        n.extend(100);
        assert!(n.is_visited(0));
        assert!(n.is_visited(1));

        for i in 2..100 {
            assert!(!n.is_visited(i));
            assert!(!n.visit(i));
            assert!(n.is_visited(i));
            assert!(n.visit(i));
            assert!(n.is_visited(i));
        }
    }
}
