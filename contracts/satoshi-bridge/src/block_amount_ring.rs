use crate::{near, require, Config, Contract};

pub const BLOCK_AMOUNT_RING_CAPACITY_SLACK: usize = 5;

#[near(serializers = [borsh])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Eq))]
pub struct BlockAmountCell {
    pub block_height: u64,
    pub cumulative_sats: u128,
}

/// Fixed-capacity ring of cumulative bridged satoshi amounts per BTC block
/// (slot = `block_height % capacity`), so confirmations tiers apply to the
/// per-block sum rather than to each tx separately.
#[near(serializers = [borsh])]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq, Eq))]
pub struct BlockAmountRing {
    cells: Vec<Option<BlockAmountCell>>,
}

impl BlockAmountRing {
    pub fn capacity_for(config: &Config) -> usize {
        usize::from(config.max_tier_confirmations())
            + usize::from(config.confirmations_delta)
            + usize::from(config.extra_msg_confirmations_delta)
            + BLOCK_AMOUNT_RING_CAPACITY_SLACK
    }

    pub fn new(capacity: usize) -> Self {
        require!(capacity > 0, "BlockAmountRing capacity must be > 0");
        Self {
            cells: vec![None; capacity],
        }
    }

    pub fn capacity(&self) -> usize {
        self.cells.len()
    }

    /// Returns the post-bump cumulative amount, or `None` if the slot holds a
    /// newer block — `block_height` is then out of the active window and the
    /// caller must require max-tier confirmations.
    pub fn bump(&mut self, block_height: u64, amount: u128) -> Option<u128> {
        let i = self.slot(block_height);
        match &mut self.cells[i] {
            Some(c) if c.block_height == block_height => {
                c.cumulative_sats = c.cumulative_sats.saturating_add(amount);
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

    pub fn get(&self, block_height: u64) -> Option<u128> {
        let i = self.slot(block_height);
        match &self.cells[i] {
            Some(c) if c.block_height == block_height => Some(c.cumulative_sats),
            _ => None,
        }
    }

    pub fn resize(&mut self, new_capacity: usize) {
        require!(new_capacity > 0, "BlockAmountRing capacity must be > 0");
        if new_capacity == self.cells.len() {
            return;
        }
        let new_cap_u64 = u64::try_from(new_capacity).expect("capacity fits u64");
        let mut new_cells: Vec<Option<BlockAmountCell>> = vec![None; new_capacity];
        for entry in self.cells.iter().flatten() {
            let i =
                usize::try_from(entry.block_height % new_cap_u64).expect("slot index fits usize");
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
    /// Panics unless the observed depth satisfies the confirmations tier for
    /// the block's post-bump cumulative amount. Out-of-window blocks fall back
    /// to the max tier.
    pub(crate) fn bump_and_check_confirmations(
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
        let actual = tip_height.saturating_sub(block_height).saturating_add(1);
        require!(
            actual >= required,
            "Not enough confirmations for the block-cumulative bridge amount"
        );
    }

    pub(crate) fn resize_block_amount_ring(&mut self) {
        let cap = BlockAmountRing::capacity_for(self.internal_config());
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
        assert_eq!(ring.bump(104, 999), Some(999));
        assert_eq!(ring.get(104), Some(999));
        assert_eq!(ring.get(100), None);
    }

    #[test]
    fn bumping_older_block_when_newer_present_returns_none() {
        let mut ring = BlockAmountRing::new(4);
        ring.bump(104, 999);
        assert_eq!(ring.bump(100, 500), None);
        assert_eq!(ring.get(104), Some(999));
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
        assert_eq!(ring.get(200), None);
        assert_eq!(ring.get(100), Some(500));
    }

    #[test]
    fn bump_overflow_saturates() {
        let mut ring = BlockAmountRing::new(4);
        ring.bump(100, u128::MAX);
        assert_eq!(ring.bump(100, 1), Some(u128::MAX));
        assert_eq!(ring.get(100), Some(u128::MAX));
    }

    #[test]
    fn capacity_one_works() {
        let mut ring = BlockAmountRing::new(1);
        ring.bump(100, 10);
        assert_eq!(ring.get(100), Some(10));
        ring.bump(101, 20);
        assert_eq!(ring.get(100), None);
        assert_eq!(ring.get(101), Some(20));
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
        ring.bump(100, 500);
        ring.bump(104, 999);
        ring.resize(4);
        assert_eq!(ring.capacity(), 4);
        assert_eq!(ring.get(104), Some(999));
        assert_eq!(ring.get(100), None);
    }

    #[test]
    fn resize_shrink_drops_only_collided_entries() {
        let mut ring = BlockAmountRing::new(8);
        ring.bump(100, 500);
        ring.bump(101, 600);
        ring.bump(102, 700);
        ring.bump(104, 800);
        ring.resize(4);
        assert_eq!(ring.get(104), Some(800));
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

    mod dao_resize {
        use crate::block_amount_ring::BLOCK_AMOUNT_RING_CAPACITY_SLACK;
        use crate::*;

        // The unit-env fixture config: strategy {10000000: 2}, confirmations_delta = 1,
        // extra_msg_confirmations_delta = 1 → capacity = 2 + 1 + 1 + slack.
        const FIXTURE_CAPACITY: usize = 2 + 1 + 1 + BLOCK_AMOUNT_RING_CAPACITY_SLACK;

        fn dao_env() -> UnitEnv {
            let mut unit_env = init_unit_env();
            testing_env!(unit_env
                .context
                .predecessor_account_id(owner_id())
                .attached_deposit(NearToken::from_yoctonear(1))
                .build());
            unit_env
        }

        fn ring_capacity(contract: &Contract) -> usize {
            contract.data().block_bridge_amounts.capacity()
        }

        #[test]
        fn set_confirmations_strategy_resizes_ring_and_preserves_entries() {
            let mut unit_env = dao_env();
            assert_eq!(ring_capacity(&unit_env.contract), FIXTURE_CAPACITY);
            unit_env
                .contract
                .data_mut()
                .block_bridge_amounts
                .bump(100, 500);

            unit_env
                .contract
                .set_confirmations_strategy(U128(20_000_000), 6);

            let expected = 6 + 1 + 1 + BLOCK_AMOUNT_RING_CAPACITY_SLACK;
            assert_eq!(ring_capacity(&unit_env.contract), expected);
            assert_eq!(
                ring_capacity(&unit_env.contract),
                BlockAmountRing::capacity_for(unit_env.contract.internal_config())
            );
            assert_eq!(
                unit_env.contract.data().block_bridge_amounts.get(100),
                Some(500)
            );
        }

        #[test]
        fn remove_confirmations_strategy_shrinks_ring_keeping_newer_on_collision() {
            let mut unit_env = dao_env();
            unit_env
                .contract
                .set_confirmations_strategy(U128(20_000_000), 6);
            let grown_capacity = ring_capacity(&unit_env.contract);
            assert!(grown_capacity > FIXTURE_CAPACITY);

            // 9 and 18 occupy distinct slots at the grown capacity (13) but the
            // same slot at the fixture capacity (9), so the shrink must collide.
            unit_env
                .contract
                .data_mut()
                .block_bridge_amounts
                .bump(9, 100);
            unit_env
                .contract
                .data_mut()
                .block_bridge_amounts
                .bump(18, 200);

            unit_env
                .contract
                .remove_confirmations_strategy(U128(20_000_000));

            assert_eq!(ring_capacity(&unit_env.contract), FIXTURE_CAPACITY);
            assert_eq!(
                ring_capacity(&unit_env.contract),
                BlockAmountRing::capacity_for(unit_env.contract.internal_config())
            );
            assert_eq!(
                unit_env.contract.data().block_bridge_amounts.get(18),
                Some(200)
            );
            assert_eq!(unit_env.contract.data().block_bridge_amounts.get(9), None);
        }

        #[test]
        fn update_config_deltas_resize_ring() {
            let mut unit_env = dao_env();
            assert_eq!(ring_capacity(&unit_env.contract), FIXTURE_CAPACITY);
            unit_env.contract.data_mut().block_bridge_amounts.bump(7, 1);

            let update: ConfigUpdate =
                serde_json::from_str(r#"{ "confirmations_delta": 4 }"#).unwrap();
            unit_env.contract.update_config(update);
            assert_eq!(
                ring_capacity(&unit_env.contract),
                2 + 4 + 1 + BLOCK_AMOUNT_RING_CAPACITY_SLACK
            );

            let update: ConfigUpdate =
                serde_json::from_str(r#"{ "extra_msg_confirmations_delta": 3 }"#).unwrap();
            unit_env.contract.update_config(update);
            assert_eq!(
                ring_capacity(&unit_env.contract),
                2 + 4 + 3 + BLOCK_AMOUNT_RING_CAPACITY_SLACK
            );
            assert_eq!(
                ring_capacity(&unit_env.contract),
                BlockAmountRing::capacity_for(unit_env.contract.internal_config())
            );

            let update: ConfigUpdate = serde_json::from_str(
                r#"{ "confirmations_delta": 0, "extra_msg_confirmations_delta": 0 }"#,
            )
            .unwrap();
            unit_env.contract.update_config(update);
            assert_eq!(
                ring_capacity(&unit_env.contract),
                2 + BLOCK_AMOUNT_RING_CAPACITY_SLACK
            );
            assert_eq!(
                ring_capacity(&unit_env.contract),
                BlockAmountRing::capacity_for(unit_env.contract.internal_config())
            );
            assert_eq!(
                unit_env.contract.data().block_bridge_amounts.get(7),
                Some(1)
            );
        }

        #[test]
        fn update_config_without_capacity_inputs_keeps_ring() {
            let mut unit_env = dao_env();
            assert_eq!(ring_capacity(&unit_env.contract), FIXTURE_CAPACITY);
            unit_env
                .contract
                .data_mut()
                .block_bridge_amounts
                .bump(5, 42);

            let update: ConfigUpdate =
                serde_json::from_str(r#"{ "min_deposit_amount": "1000" }"#).unwrap();
            unit_env.contract.update_config(update);

            assert_eq!(ring_capacity(&unit_env.contract), FIXTURE_CAPACITY);
            assert_eq!(
                unit_env.contract.data().block_bridge_amounts.get(5),
                Some(42)
            );
        }

        #[test]
        fn set_confirmations_strategy_middle_tier_keeps_ring() {
            let mut unit_env = dao_env();
            unit_env
                .contract
                .set_confirmations_strategy(U128(20_000_000), 6);
            let grown_capacity = ring_capacity(&unit_env.contract);
            unit_env
                .contract
                .data_mut()
                .block_bridge_amounts
                .bump(11, 300);

            unit_env
                .contract
                .set_confirmations_strategy(U128(15_000_000), 4);

            assert_eq!(ring_capacity(&unit_env.contract), grown_capacity);
            assert_eq!(
                ring_capacity(&unit_env.contract),
                BlockAmountRing::capacity_for(unit_env.contract.internal_config())
            );
            assert_eq!(
                unit_env.contract.data().block_bridge_amounts.get(11),
                Some(300)
            );
        }
    }
}
