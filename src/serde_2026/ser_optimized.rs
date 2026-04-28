use super::ser::SerializerState;
use super::strategy::{Direction, VisitStrategy};

/// Tree-DP strategy: pick left-first vs right-first per pair to maximise the
/// number of pair back-references that fit in the 1-byte varint tier.
///
/// Complexity: O(N x min(subtree_size, 64)) to build; O(1) per pair at
/// emit time.
pub(super) struct DpOptimized {
    stsize: Vec<usize>,
    go_left: Vec<Vec<bool>>,
}

impl DpOptimized {
    const TIER1_SLOTS: usize = 63;

    pub fn new(state: &SerializerState) -> Self {
        let pair_count = state.tree.pairs.len();

        let mut pair_ref_count = vec![0u32; pair_count];
        if state.root_index < 0 {
            pair_ref_count[(-state.root_index - 1) as usize] += 1;
        }
        for &(left, right) in &state.pairs {
            for idx in [left, right] {
                if idx < 0 {
                    pair_ref_count[(-idx - 1) as usize] += 1;
                }
            }
        }

        let mut stsize = vec![0usize; pair_count];
        let mut dp: Vec<Vec<u64>> = Vec::with_capacity(pair_count);
        let mut go_left: Vec<Vec<bool>> = Vec::with_capacity(pair_count);

        for (i, &(left, right)) in state.pairs.iter().enumerate() {
            let savings: u64 = if pair_ref_count[i] > 1 {
                (pair_ref_count[i] - 1) as u64
            } else {
                0
            };

            let l_size = if left < 0 {
                stsize[(-left - 1) as usize]
            } else {
                0
            };
            let r_size = if right < 0 {
                stsize[(-right - 1) as usize]
            } else {
                0
            };
            let my_size = 1 + l_size + r_size;
            stsize[i] = my_size;

            let max_k = my_size.min(Self::TIER1_SLOTS + 1);
            let mut my_dp = vec![0u64; max_k + 1];
            let mut my_go = vec![true; max_k + 1];

            let child_dp = |child: i32, budget: usize| -> u64 {
                if child >= 0 {
                    return 0;
                }
                let ci = (-child - 1) as usize;
                let arr = &dp[ci];
                arr[budget.min(arr.len() - 1)]
            };

            for k in 1..=max_k {
                let p_cheap = k >= my_size;
                let cb = if p_cheap { k - 1 } else { k };

                let l_lf = cb.min(l_size);
                let r_lf = (cb - l_lf).min(r_size);
                let val_lf = child_dp(left, l_lf) + child_dp(right, r_lf);

                let r_rf = cb.min(r_size);
                let l_rf = (cb - r_rf).min(l_size);
                let val_rf = child_dp(left, l_rf) + child_dp(right, r_rf);

                let p_sav = if p_cheap { savings } else { 0 };
                if val_lf >= val_rf {
                    my_dp[k] = p_sav + val_lf;
                    my_go[k] = true;
                } else {
                    my_dp[k] = p_sav + val_rf;
                    my_go[k] = false;
                }
            }

            dp.push(my_dp);
            go_left.push(my_go);
        }

        drop(dp);
        Self { stsize, go_left }
    }
}

impl VisitStrategy for DpOptimized {
    type NodeCtx = usize;

    fn root_ctx(&self, state: &SerializerState) -> usize {
        Self::TIER1_SLOTS.min(if state.root_index < 0 {
            self.stsize[(-state.root_index - 1) as usize]
        } else {
            0
        })
    }

    fn decide(
        &self,
        state: &SerializerState,
        pair_idx: usize,
        budget: usize,
    ) -> (Direction, usize, usize) {
        let (left, right) = state.pairs[pair_idx];
        let l_size = if left < 0 {
            self.stsize[(-left - 1) as usize]
        } else {
            0
        };
        let r_size = if right < 0 {
            self.stsize[(-right - 1) as usize]
        } else {
            0
        };
        let my_size = 1 + l_size + r_size;

        let k = budget.min(my_size).min(self.go_left[pair_idx].len() - 1);
        let p_cheap = k >= my_size;
        let cb = if p_cheap { k - 1 } else { k };

        if self.go_left[pair_idx][k] {
            let l_b = cb.min(l_size);
            let r_b = (cb - l_b).min(r_size);
            (Direction::LeftFirst, l_b, r_b)
        } else {
            let r_b = cb.min(r_size);
            let l_b = (cb - r_b).min(l_size);
            (Direction::RightFirst, l_b, r_b)
        }
    }
}
