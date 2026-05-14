use crate::{near, require, Contract};

#[near(serializers = [borsh])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Eq))]
pub struct BlockAmountCell {
    pub block_height: u64,
    pub cumulative_sats: u128,
}

/// Fixed-capacity ring buffer of cumulative bridge-related satoshi amounts per BTC block.
/// Each slot is `cells[block_height % capacity]` and stores the block_height explicitly
/// so a stale entry can be detected and lazily overwritten when a new block lands in the
/// same slot. Capacity must cover any block whose confirmations could still be in question
/// for tier checks; blocks deeper than capacity are out of the active window and the caller
/// must require max-tier confirmations for them (which they trivially satisfy by depth).
#[near(serializers = [borsh])]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Eq))]
pub struct BlockAmountRing {
    cells: Vec<Option<BlockAmountCell>>,
}

impl BlockAmountRing {
    pub fn new(capacity: usize) -> Self {
        require!(capacity > 0, "BlockAmountRing capacity must be > 0");
        Self {
            cells: vec![None; capacity],
        }
    }

    pub fn capacity(&self) -> usize {
        self.cells.len()
    }

    /// Add `amount` to the cumulative bridge sats for `block_height` and return
    /// `Some(post_bump)`.
    ///
    /// Returns `None` if the target slot holds a NEWER block — `block_height` is
    /// outside the active tracking window and must not corrupt the newer block's
    /// data. Callers should treat `None` as "out of window — require max-tier
    /// confirmations" (which is trivially satisfied by depth at that point).
    ///
    /// If the slot is empty or holds an OLDER block (now out of window), it is
    /// overwritten with `(block_height, amount)` — lazy eviction.
    pub fn bump(&mut self, block_height: u64, amount: u128) -> Option<u128> {
        let i = self.slot(block_height);
        match &mut self.cells[i] {
            Some(c) if c.block_height == block_height => {
                c.cumulative_sats = c
                    .cumulative_sats
                    .checked_add(amount)
                    .expect("BlockAmountRing: cumulative_sats overflow");
                Some(c.cumulative_sats)
            }
            Some(c) if c.block_height > block_height => None,
            cell => {
                *cell = Some(BlockAmountCell {
                    block_height,
                    cumulative_sats: amount,
                });
                Some(amount)
            }
        }
    }

    /// Cumulative bridge sats currently stored for `block_height`, or `None` if the slot
    /// is empty or holds a different block (caller treats `None` as "out of active window —
    /// require max-tier confirmations").
    pub fn get(&self, block_height: u64) -> Option<u128> {
        let i = self.slot(block_height);
        match &self.cells[i] {
            Some(c) if c.block_height == block_height => Some(c.cumulative_sats),
            _ => None,
        }
    }

    /// Re-allocate to `new_capacity`, rehashing existing entries to `block_height % new_capacity`.
    /// On collisions, the entry with the larger `block_height` (newer block) wins; older entries
    /// are dropped. Capacity-preserving resize is a no-op.
    pub fn resize(&mut self, new_capacity: usize) {
        require!(new_capacity > 0, "BlockAmountRing capacity must be > 0");
        if new_capacity == self.cells.len() {
            return;
        }
        let new_cap_u64 = u64::try_from(new_capacity).expect("capacity fits u64");
        let mut new_cells: Vec<Option<BlockAmountCell>> = vec![None; new_capacity];
        for entry in self.cells.iter().flatten() {
            let i = usize::try_from(entry.block_height % new_cap_u64)
                .expect("slot index fits usize");
            let replace = match &new_cells[i] {
                Some(existing) => entry.block_height > existing.block_height,
                None => true,
            };
            if replace {
                new_cells[i] = Some(entry.clone());
            }
        }
        self.cells = new_cells;
    }

    fn slot(&self, block_height: u64) -> usize {
        let cap = u64::try_from(self.capacity()).expect("capacity fits u64");
        usize::try_from(block_height % cap).expect("slot index fits usize")
    }
}

impl Contract {
    /// Bump the block-amount ring with `amount` for `block_height` and require
    /// that the observed depth `tip - height + 1` satisfies the confirmations
    /// tier for the resulting cumulative amount. `delta` is the pre-computed
    /// whitelist delta from the entry point (the callback's predecessor is
    /// this contract, not the original relayer, so the whitelist check must
    /// be done up-front).
    ///
    /// If `block_height` is outside the active ring window, the ring refuses
    /// the bump and we fall back to max-tier — which is trivially satisfied
    /// by depth at that point, so the check passes.
    ///
    /// Panics if not enough confirmations.
    pub fn bump_and_check_confirmations(
        &mut self,
        block_height: u64,
        tip_height: u64,
        amount: u128,
        delta: u64,
    ) {
        let cumulative = self
            .data_mut()
            .block_bridge_amounts
            .bump(block_height, amount)
            .unwrap_or(u128::MAX);
        let required = self.internal_config().get_confirmations(cumulative) + delta;
        let actual = tip_height.saturating_sub(block_height) + 1;
        require!(
            actual >= required,
            "Not enough confirmations for the block-cumulative bridge amount"
        );
    }

    /// Resize the block-amount ring to match the current config's capacity.
    /// Called after config updates that change `confirmations_delta`,
    /// `extra_msg_confirmations_delta`, or the tier table.
    pub fn resize_block_amount_ring(&mut self) {
        let cap = self.internal_config().block_amount_ring_capacity();
        self.data_mut().block_bridge_amounts.resize(cap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ring_is_empty() {
        let ring = BlockAmountRing::new(8);
        assert_eq!(ring.capacity(), 8);
        for h in 0..100u64 {
            assert_eq!(ring.get(h), None);
        }
    }

    #[test]
    #[should_panic(expected = "BlockAmountRing capacity must be > 0")]
    fn new_with_zero_capacity_panics() {
        BlockAmountRing::new(0);
    }

    #[test]
    fn bump_then_get_returns_amount() {
        let mut ring = BlockAmountRing::new(4);
        assert_eq!(ring.bump(100, 500), Some(500));
        assert_eq!(ring.get(100), Some(500));
    }

    #[test]
    fn bumping_same_height_accumulates() {
        let mut ring = BlockAmountRing::new(4);
        ring.bump(100, 500);
        assert_eq!(ring.bump(100, 250), Some(750));
        assert_eq!(ring.bump(100, 1), Some(751));
        assert_eq!(ring.get(100), Some(751));
    }

    #[test]
    fn bumping_newer_block_same_slot_evicts_older() {
        let mut ring = BlockAmountRing::new(4);
        ring.bump(100, 500);
        // 100 and 104 collide (both 100 % 4 == 0); 104 is newer → evict 100.
        assert_eq!(ring.bump(104, 999), Some(999));
        assert_eq!(ring.get(104), Some(999));
        // The previous block's data is gone
        assert_eq!(ring.get(100), None);
    }

    #[test]
    fn bumping_older_block_when_newer_present_returns_none() {
        let mut ring = BlockAmountRing::new(4);
        // 104 is newer; bumping older 100 into the same slot must not corrupt 104's data.
        ring.bump(104, 999);
        assert_eq!(ring.bump(100, 500), None);
        // Newer block's data still intact
        assert_eq!(ring.get(104), Some(999));
        // Older block was not stored — out of active window
        assert_eq!(ring.get(100), None);
    }

    #[test]
    fn distinct_heights_in_distinct_slots_do_not_interfere() {
        let mut ring = BlockAmountRing::new(8);
        ring.bump(100, 1);
        ring.bump(101, 2);
        ring.bump(102, 3);
        assert_eq!(ring.get(100), Some(1));
        assert_eq!(ring.get(101), Some(2));
        assert_eq!(ring.get(102), Some(3));
    }

    #[test]
    fn get_returns_none_for_unknown_height_in_used_slot() {
        let mut ring = BlockAmountRing::new(4);
        ring.bump(100, 500);
        // 200 hits the same slot as 100 but no data was stored for height 200
        assert_eq!(ring.get(200), None);
        // Original entry still intact
        assert_eq!(ring.get(100), Some(500));
    }

    #[test]
    #[should_panic(expected = "cumulative_sats overflow")]
    fn bump_overflow_panics() {
        let mut ring = BlockAmountRing::new(4);
        ring.bump(100, u128::MAX);
        ring.bump(100, 1);
    }

    #[test]
    fn capacity_one_works() {
        let mut ring = BlockAmountRing::new(1);
        ring.bump(100, 10);
        assert_eq!(ring.get(100), Some(10));
        // Newer height evicts older
        ring.bump(101, 20);
        assert_eq!(ring.get(100), None);
        assert_eq!(ring.get(101), Some(20));
        // Older height after newer is refused
        assert_eq!(ring.bump(100, 9), None);
        assert_eq!(ring.get(101), Some(20));
        assert_eq!(ring.get(100), None);
    }

    #[test]
    fn resize_same_capacity_is_noop() {
        let mut ring = BlockAmountRing::new(4);
        ring.bump(100, 500);
        ring.bump(101, 700);
        ring.resize(4);
        assert_eq!(ring.get(100), Some(500));
        assert_eq!(ring.get(101), Some(700));
    }

    #[test]
    fn resize_grow_preserves_all_entries() {
        let mut ring = BlockAmountRing::new(4);
        ring.bump(100, 500);
        ring.bump(101, 700);
        ring.bump(102, 900);
        ring.resize(8);
        assert_eq!(ring.capacity(), 8);
        assert_eq!(ring.get(100), Some(500));
        assert_eq!(ring.get(101), Some(700));
        assert_eq!(ring.get(102), Some(900));
    }

    #[test]
    fn resize_grow_no_collisions_when_new_cap_geq_height_range() {
        let mut ring = BlockAmountRing::new(4);
        // Fill all slots with distinct heights
        ring.bump(100, 1);
        ring.bump(101, 2);
        ring.bump(102, 3);
        ring.bump(103, 4);
        ring.resize(8);
        for (h, expected) in [(100u64, 1u128), (101, 2), (102, 3), (103, 4)] {
            assert_eq!(ring.get(h), Some(expected));
        }
    }

    #[test]
    fn resize_shrink_keeps_newer_block_on_collision() {
        let mut ring = BlockAmountRing::new(8);
        // 100 and 104 do NOT collide in cap=8 (100 % 8 == 4, 104 % 8 == 0).
        ring.bump(100, 500);
        ring.bump(104, 999);
        // After shrinking to cap=4: 100 % 4 == 0, 104 % 4 == 0 — collision.
        ring.resize(4);
        assert_eq!(ring.capacity(), 4);
        // Newer block wins
        assert_eq!(ring.get(104), Some(999));
        assert_eq!(ring.get(100), None);
    }

    #[test]
    fn resize_shrink_drops_only_collided_entries() {
        let mut ring = BlockAmountRing::new(8);
        ring.bump(100, 500); // 100 % 8 == 4, 100 % 4 == 0
        ring.bump(101, 600); // 101 % 8 == 5, 101 % 4 == 1
        ring.bump(102, 700); // 102 % 8 == 6, 102 % 4 == 2
        ring.bump(104, 800); // 104 % 8 == 0, 104 % 4 == 0 — will collide with 100
        ring.resize(4);
        assert_eq!(ring.get(104), Some(800)); // newer wins over 100
        assert_eq!(ring.get(100), None);
        assert_eq!(ring.get(101), Some(600));
        assert_eq!(ring.get(102), Some(700));
    }

    #[test]
    #[should_panic(expected = "BlockAmountRing capacity must be > 0")]
    fn resize_to_zero_panics() {
        let mut ring = BlockAmountRing::new(4);
        ring.resize(0);
    }

    #[test]
    fn resize_preserves_cumulative_within_kept_block() {
        let mut ring = BlockAmountRing::new(4);
        ring.bump(100, 100);
        ring.bump(100, 200);
        ring.bump(100, 300);
        assert_eq!(ring.get(100), Some(600));
        ring.resize(8);
        assert_eq!(ring.get(100), Some(600));
    }
}
