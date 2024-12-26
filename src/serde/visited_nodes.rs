/// This is a simple bitfield used to indicates whether a node has been visited
/// during a tree search or not. We terminate a search path if we've reached the
/// node first via a different (shorter) path.
#[derive(Clone, Default)]
pub struct VisitedNodes {
    bits: Vec<usize>,
}

impl VisitedNodes {
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
        let mut n = VisitedNodes::new(100);
        for i in 0..100 {
            assert_eq!(n.is_visited(i), false);
            assert_eq!(n.visit(i), false);
            assert_eq!(n.is_visited(i), true);
            assert_eq!(n.visit(i), true);
            assert_eq!(n.is_visited(i), true);
        }
    }

    #[test]
    fn test_visited_nodes_reverse() {
        let mut n = VisitedNodes::new(100);
        for i in (0..100).rev() {
            assert_eq!(n.is_visited(i), false);
            assert_eq!(n.visit(i), false);
            assert_eq!(n.is_visited(i), true);
            assert_eq!(n.visit(i), true);
            assert_eq!(n.is_visited(i), true);
        }
    }

    #[test]
    fn test_extend() {
        let mut n = VisitedNodes::default();
        n.extend(1);
        assert_eq!(n.is_visited(0), false);
        assert_eq!(n.visit(0), false);
        assert_eq!(n.is_visited(0), true);

        n.extend(2);
        assert_eq!(n.is_visited(0), true);

        assert_eq!(n.is_visited(1), false);
        assert_eq!(n.visit(1), false);
        assert_eq!(n.is_visited(1), true);

        n.extend(100);
        assert_eq!(n.is_visited(0), true);
        assert_eq!(n.is_visited(1), true);

        for i in 2..100 {
            assert_eq!(n.is_visited(i), false);
            assert_eq!(n.visit(i), false);
            assert_eq!(n.is_visited(i), true);
            assert_eq!(n.visit(i), true);
            assert_eq!(n.is_visited(i), true);
        }
    }
}
