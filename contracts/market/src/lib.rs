use near_sdk::env;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::store::{LookupMap, LookupSet};
use near_sdk::{near, require, AccountId, BorshStorageKey, PanicOnDefault};

pub const MIN_TIMELOCK_SECS: u64 = 60;
pub const INITIAL_INTEREST_INDEX: u128 = 10u128.pow(18);

#[derive(BorshStorageKey)]
#[near(serializers = [borsh])]
enum StorageKey {
    Assets,
    AssetKeys,
    PendingParamChanges,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainFamily {
    Evm,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetKind {
    NativeEth,
    Nep141,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub kind: AssetKind,
    pub account: String,
    pub decimals: u8,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterestCurve {
    pub base_bps: u16,
    pub slope_bps: u16,
    pub kink_utilization_bps: u16,
    pub kink_slope_bps: u16,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct AssetConfig {
    pub asset: Asset,
    pub family: ChainFamily,
    pub chain_id: u64,
    pub light_client: AccountId,
    pub supply_enabled: bool,
    pub collateral_enabled: bool,
    pub collateral_factor_bps: u16,
    pub liquidation_threshold_bps: u16,
    pub liquidation_penalty_bps: u16,
    pub reserve_factor_bps: u16,
    pub interest_curve: InterestCurve,
    pub min_deposit: U128,
    pub min_borrow: U128,
    pub supply_cap: U128,
    pub borrow_cap: U128,
    pub price_feed: AccountId,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct AssetPoolState {
    pub total_supply: U128,
    pub total_borrow: U128,
    pub total_reserves: U128,
    pub interest_index: U128,
    pub last_accrual_secs: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct AssetEntry {
    pub config: AssetConfig,
    pub pool: AssetPoolState,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamUpdate {
    CollateralFactorBps(u16),
    LiquidationThresholdBps(u16),
    LiquidationPenaltyBps(u16),
    ReserveFactorBps(u16),
    SupplyCap(U128),
    BorrowCap(U128),
    MinBorrow(U128),
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct PendingParamChange {
    pub update: ParamUpdate,
    pub execute_at_secs: u64,
}

#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AssetKey {
    chain_id: u64,
    account: String,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Market {
    owner: AccountId,
    timelock_secs: u64,
    next_asset_id: u32,
    assets: LookupMap<u32, AssetEntry>,
    asset_keys: LookupSet<AssetKey>,
    pending: LookupMap<u32, PendingParamChange>,
}

fn validate_asset(asset: &Asset, message: &'static str) {
    match asset.kind {
        AssetKind::NativeEth => {
            require!(asset.account == "native", message);
            require!(asset.decimals == 18, message);
        }
        AssetKind::Nep141 => {
            require!(asset.account.parse::<AccountId>().is_ok(), message);
            require!(asset.decimals <= 18, message);
        }
    }
}

fn validate_asset_config(config: &AssetConfig) {
    require!(config.family == ChainFamily::Evm, "unsupported chain");
    require!(config.chain_id != 0, "invalid chain config");
    match config.asset.kind {
        AssetKind::NativeEth => {
            require!(!config.supply_enabled, "native eth is collateral only");
        }
        AssetKind::Nep141 => {}
    }
    require!(
        config.supply_enabled || config.collateral_enabled,
        "asset must enable supply or collateral"
    );
    validate_asset(&config.asset, "invalid asset");
    require!(
        config.collateral_factor_bps > 0
            && config.collateral_factor_bps <= config.liquidation_threshold_bps
            && config.liquidation_threshold_bps <= 10_000,
        "invalid market economics"
    );
    require!(
        config.liquidation_penalty_bps <= 5_000,
        "invalid market economics"
    );
    require!(
        config.reserve_factor_bps < 10_000,
        "invalid market economics"
    );
    let curve = &config.interest_curve;
    require!(
        curve.base_bps <= 10_000
            && curve.slope_bps <= 10_000
            && curve.kink_slope_bps <= 10_000
            && curve.kink_utilization_bps <= 10_000,
        "invalid interest curve"
    );
    require!(config.min_borrow.0 > 0, "invalid market economics");
    require!(
        config.supply_cap.0 > 0 && config.borrow_cap.0 > 0,
        "invalid caps"
    );
    require!(config.supply_cap.0 >= config.min_deposit.0, "invalid caps");
    require!(config.borrow_cap.0 >= config.min_borrow.0, "invalid caps");
}

impl ParamUpdate {
    fn apply_to(&self, config: &mut AssetConfig) {
        match self {
            ParamUpdate::CollateralFactorBps(value) => config.collateral_factor_bps = *value,
            ParamUpdate::LiquidationThresholdBps(value) => {
                config.liquidation_threshold_bps = *value
            }
            ParamUpdate::LiquidationPenaltyBps(value) => config.liquidation_penalty_bps = *value,
            ParamUpdate::ReserveFactorBps(value) => config.reserve_factor_bps = *value,
            ParamUpdate::SupplyCap(value) => config.supply_cap = *value,
            ParamUpdate::BorrowCap(value) => config.borrow_cap = *value,
            ParamUpdate::MinBorrow(value) => config.min_borrow = *value,
        }
    }
}

fn now_secs() -> u64 {
    env::block_timestamp() / 1_000_000_000
}

#[near]
impl Market {
    #[init]
    pub fn new(owner: AccountId, timelock_secs: u64) -> Self {
        require!(timelock_secs >= MIN_TIMELOCK_SECS, "timelock too short");
        Self {
            owner,
            timelock_secs,
            next_asset_id: 1,
            assets: LookupMap::new(StorageKey::Assets),
            asset_keys: LookupSet::new(StorageKey::AssetKeys),
            pending: LookupMap::new(StorageKey::PendingParamChanges),
        }
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    pub fn get_timelock_secs(&self) -> u64 {
        self.timelock_secs
    }

    pub fn get_asset_count(&self) -> u32 {
        self.next_asset_id - 1
    }

    pub fn get_asset(&self, asset_id: u32) -> AssetEntry {
        self.assets.get(&asset_id).expect("asset not found").clone()
    }

    pub fn get_pending_param_change(&self, asset_id: u32) -> Option<PendingParamChange> {
        self.pending.get(&asset_id).cloned()
    }

    pub fn register_asset(&mut self, config: AssetConfig) -> u32 {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        validate_asset_config(&config);
        let key = AssetKey {
            chain_id: config.chain_id,
            account: config.asset.account.clone(),
        };
        require!(self.asset_keys.insert(key), "asset already exists");
        let asset_id = self.next_asset_id;
        let chain_id = config.chain_id;
        let account = config.asset.account.clone();
        self.assets.insert(
            asset_id,
            AssetEntry {
                config,
                pool: AssetPoolState {
                    total_supply: U128(0),
                    total_borrow: U128(0),
                    total_reserves: U128(0),
                    interest_index: U128(INITIAL_INTEREST_INDEX),
                    last_accrual_secs: now_secs(),
                },
            },
        );
        self.next_asset_id += 1;
        env::log_str(
            &json!({
                "event": "asset_registered",
                "asset_id": asset_id,
                "chain_id": chain_id,
                "asset": account,
            })
            .to_string(),
        );
        asset_id
    }

    pub fn propose_param_update(&mut self, asset_id: u32, update: ParamUpdate) {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        require!(
            self.pending.get(&asset_id).is_none(),
            "pending change exists"
        );
        let mut entry = self.assets.get(&asset_id).expect("asset not found").clone();
        update.apply_to(&mut entry.config);
        validate_asset_config(&entry.config);
        self.pending.insert(
            asset_id,
            PendingParamChange {
                update,
                execute_at_secs: now_secs() + self.timelock_secs,
            },
        );
        env::log_str(
            &json!({
                "event": "param_update_proposed",
                "asset_id": asset_id,
                "execute_at_secs": now_secs() + self.timelock_secs,
            })
            .to_string(),
        );
    }

    pub fn cancel_param_update(&mut self, asset_id: u32) {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        require!(self.pending.get(&asset_id).is_some(), "no pending change");
        self.pending.remove(&asset_id);
        env::log_str(
            &json!({
                "event": "param_update_cancelled",
                "asset_id": asset_id,
            })
            .to_string(),
        );
    }

    pub fn confirm_param_update(&mut self, asset_id: u32) {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        let pending = self
            .pending
            .get(&asset_id)
            .expect("no pending change")
            .clone();
        require!(
            now_secs() >= pending.execute_at_secs,
            "timelock not elapsed"
        );
        let mut entry = self.assets.get(&asset_id).expect("asset not found").clone();
        pending.update.apply_to(&mut entry.config);
        validate_asset_config(&entry.config);
        self.assets.insert(asset_id, entry);
        self.pending.remove(&asset_id);
        env::log_str(
            &json!({
                "event": "param_update_applied",
                "asset_id": asset_id,
            })
            .to_string(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn context(predecessor: &str, timestamp_secs: u64) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor.parse().unwrap())
            .block_timestamp(timestamp_secs * 1_000_000_000);
        builder
    }

    fn token_config(account: &str, supply: bool, collateral: bool) -> AssetConfig {
        AssetConfig {
            asset: Asset {
                kind: AssetKind::Nep141,
                account: account.to_string(),
                decimals: 6,
            },
            family: ChainFamily::Evm,
            chain_id: 11_155_111,
            light_client: "client-eth2.sepolia.testnet".parse().unwrap(),
            supply_enabled: supply,
            collateral_enabled: collateral,
            collateral_factor_bps: 7_500,
            liquidation_threshold_bps: 8_000,
            liquidation_penalty_bps: 800,
            reserve_factor_bps: 1_000,
            interest_curve: InterestCurve {
                base_bps: 100,
                slope_bps: 1_500,
                kink_utilization_bps: 8_000,
                kink_slope_bps: 9_000,
            },
            min_deposit: U128(10u128.pow(12)),
            min_borrow: U128(1_000_000),
            supply_cap: U128(u128::MAX),
            borrow_cap: U128(u128::MAX),
            price_feed: "feed.testnet".parse().unwrap(),
        }
    }

    fn usdc_config() -> AssetConfig {
        let mut config = token_config("usdc.testnet", true, false);
        config.price_feed = "feed-usdc.testnet".parse().unwrap();
        config
    }

    fn usdt_config() -> AssetConfig {
        let mut config = token_config("usdt.testnet", true, false);
        config.price_feed = "feed-usdt.testnet".parse().unwrap();
        config
    }

    fn eth_config() -> AssetConfig {
        AssetConfig {
            asset: Asset {
                kind: AssetKind::NativeEth,
                account: "native".to_string(),
                decimals: 18,
            },
            family: ChainFamily::Evm,
            chain_id: 11_155_111,
            light_client: "client-eth2.sepolia.testnet".parse().unwrap(),
            supply_enabled: false,
            collateral_enabled: true,
            collateral_factor_bps: 7_500,
            liquidation_threshold_bps: 8_000,
            liquidation_penalty_bps: 800,
            reserve_factor_bps: 1_000,
            interest_curve: InterestCurve {
                base_bps: 0,
                slope_bps: 0,
                kink_utilization_bps: 8_000,
                kink_slope_bps: 0,
            },
            min_deposit: U128(10u128.pow(12)),
            min_borrow: U128(1_000_000),
            supply_cap: U128(u128::MAX),
            borrow_cap: U128(u128::MAX),
            price_feed: "feed-eth.testnet".parse().unwrap(),
        }
    }

    fn owner_market() -> Market {
        testing_env!(context("owner.testnet", 1_000).build());
        Market::new("owner.testnet".parse().unwrap(), 300)
    }

    fn owner_market_with(timelock_secs: u64) -> Market {
        testing_env!(context("owner.testnet", 1_000).build());
        Market::new("owner.testnet".parse().unwrap(), timelock_secs)
    }

    #[test]
    fn init_rejects_short_timelock() {
        testing_env!(context("owner.testnet", 1_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Market::new("owner.testnet".parse().unwrap(), 59);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn init_accepts_minimum_timelock() {
        let market = owner_market_with(60);
        assert_eq!(market.get_timelock_secs(), 60);
    }

    #[test]
    fn register_asset_assigns_sequential_ids() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let usdc = market.register_asset(usdc_config());
        let usdt = market.register_asset(usdt_config());
        assert_eq!(usdc, 1);
        assert_eq!(usdt, 2);
        assert_eq!(market.get_asset_count(), 2);
        let stored = market.get_asset(1);
        assert_eq!(stored.config.asset.account, "usdc.testnet");
        assert!(stored.config.supply_enabled);
        assert!(!stored.config.collateral_enabled);
        assert_eq!(stored.pool.interest_index.0, INITIAL_INTEREST_INDEX);
        assert_eq!(stored.pool.total_supply.0, 0);
    }

    #[test]
    fn register_asset_rejects_duplicate_on_chain() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_asset(usdc_config());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_asset(usdc_config());
        }));
        assert!(result.is_err());
    }

    #[test]
    fn register_asset_allows_same_asset_on_different_chain() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_asset(usdc_config());
        let mut base_usdc = usdc_config();
        base_usdc.chain_id = 84532;
        base_usdc.light_client = "client-eth2.base-sepolia.testnet".parse().unwrap();
        base_usdc.price_feed = "feed-usdc-base.testnet".parse().unwrap();
        let id = market.register_asset(base_usdc);
        assert_eq!(id, 2);
    }

    #[test]
    fn register_asset_allows_eth_collateral_and_token_collateral() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let eth = market.register_asset(eth_config());
        let collateral_usdc = {
            let mut config = token_config("usdc.testnet", true, true);
            config.price_feed = "feed-usdc.testnet".parse().unwrap();
            config
        };
        let usdc = market.register_asset(collateral_usdc);
        assert_eq!(eth, 1);
        assert_eq!(usdc, 2);
        assert!(market.get_asset(1).config.collateral_enabled);
        assert!(market.get_asset(2).config.collateral_enabled);
    }

    #[test]
    fn non_owner_cannot_register_asset() {
        let mut market = owner_market();
        testing_env!(context("attacker.testnet", 1_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_asset(usdc_config());
        }));
        assert!(result.is_err());
        assert_eq!(market.get_asset_count(), 0);
    }

    #[test]
    fn register_asset_rejects_bad_account() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_asset(token_config("not an account", true, false));
        }));
        assert!(result.is_err());
    }

    #[test]
    fn register_asset_rejects_native_eth_as_supply_asset() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let mut bad_eth = eth_config();
        bad_eth.supply_enabled = true;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_asset(bad_eth);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn register_asset_rejects_asset_without_role() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_asset(token_config("idle.testnet", false, false));
        }));
        assert!(result.is_err());
    }

    #[test]
    fn register_asset_rejects_inverted_economics() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let mut params = usdc_config();
        params.liquidation_threshold_bps = 7_000;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_asset(params);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn register_asset_rejects_bad_caps() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let mut params = usdc_config();
        params.borrow_cap = U128(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_asset(params);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn propose_rejects_change_that_breaks_economics() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_asset(usdc_config());
        testing_env!(context("owner.testnet", 2_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.propose_param_update(1, ParamUpdate::CollateralFactorBps(12_000));
        }));
        assert!(result.is_err());
        assert!(market.get_pending_param_change(1).is_none());
    }

    #[test]
    fn timelock_blocks_early_confirmation_then_applies() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_asset(usdc_config());
        testing_env!(context("owner.testnet", 2_000).build());
        market.propose_param_update(1, ParamUpdate::ReserveFactorBps(2_000));
        let pending = market.get_pending_param_change(1).expect("pending");
        assert_eq!(pending.execute_at_secs, 2_300);

        testing_env!(context("owner.testnet", 2_299).build());
        let early = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.confirm_param_update(1);
        }));
        assert!(early.is_err());
        assert!(market.get_pending_param_change(1).is_some());

        testing_env!(context("owner.testnet", 2_301).build());
        market.confirm_param_update(1);
        assert!(market.get_pending_param_change(1).is_none());
        assert_eq!(market.get_asset(1).config.reserve_factor_bps, 2_000);
    }

    #[test]
    fn cancel_clears_pending_change() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_asset(usdc_config());
        testing_env!(context("owner.testnet", 2_000).build());
        market.propose_param_update(1, ParamUpdate::MinBorrow(U128(2_000_000)));
        market.cancel_param_update(1);
        assert!(market.get_pending_param_change(1).is_none());
        testing_env!(context("owner.testnet", 3_000).build());
        let confirmed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.confirm_param_update(1);
        }));
        assert!(confirmed.is_err());
        assert_eq!(market.get_asset(1).config.min_borrow.0, 1_000_000);
    }

    #[test]
    fn confirm_requires_pending_change() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_asset(usdc_config());
        testing_env!(context("owner.testnet", 5_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.confirm_param_update(1);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn non_owner_cannot_propose_or_confirm() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_asset(usdc_config());
        testing_env!(context("attacker.testnet", 2_000).build());
        let proposed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.propose_param_update(1, ParamUpdate::ReserveFactorBps(2_000));
        }));
        assert!(proposed.is_err());
        testing_env!(context("owner.testnet", 2_100).build());
        market.propose_param_update(1, ParamUpdate::ReserveFactorBps(2_000));
        testing_env!(context("attacker.testnet", 3_000).build());
        let confirmed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.confirm_param_update(1);
        }));
        assert!(confirmed.is_err());
    }
}
