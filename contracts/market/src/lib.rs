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
    Markets,
    MarketKeys,
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
pub struct MarketParams {
    pub chain_family: ChainFamily,
    pub chain_id: u64,
    pub light_client: AccountId,
    pub collateral_asset: Option<Asset>,
    pub loan_asset: Asset,
    pub collateral_factor_bps: u16,
    pub liquidation_threshold_bps: u16,
    pub liquidation_penalty_bps: u16,
    pub reserve_factor_bps: u16,
    pub interest_curve: InterestCurve,
    pub min_deposit: U128,
    pub min_borrow: U128,
    pub supply_cap: U128,
    pub borrow_cap: U128,
    pub collateral_price_feed: AccountId,
    pub loan_price_feed: AccountId,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct PoolState {
    pub total_supply: U128,
    pub total_borrow: U128,
    pub total_reserves: U128,
    pub interest_index: U128,
    pub last_accrual_secs: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct MarketEntry {
    pub params: MarketParams,
    pub state: PoolState,
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
struct MarketKey {
    chain_id: u64,
    loan_asset: String,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Market {
    owner: AccountId,
    timelock_secs: u64,
    next_market_id: u32,
    markets: LookupMap<u32, MarketEntry>,
    market_keys: LookupSet<MarketKey>,
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

fn validate_market_params(params: &MarketParams) {
    require!(params.chain_family == ChainFamily::Evm, "unsupported chain");
    require!(params.chain_id != 0, "invalid chain config");
    require!(
        params
            .collateral_asset
            .as_ref()
            .map(|asset| asset.kind == AssetKind::NativeEth)
            .unwrap_or(false),
        "unsupported collateral asset"
    );
    if let Some(asset) = &params.collateral_asset {
        validate_asset(asset, "unsupported collateral asset");
    }
    require!(
        params.loan_asset.kind == AssetKind::Nep141,
        "invalid loan asset"
    );
    validate_asset(&params.loan_asset, "invalid loan asset");
    require!(
        params.collateral_factor_bps > 0
            && params.collateral_factor_bps <= params.liquidation_threshold_bps
            && params.liquidation_threshold_bps <= 10_000,
        "invalid market economics"
    );
    require!(
        params.liquidation_penalty_bps <= 5_000,
        "invalid market economics"
    );
    require!(
        params.reserve_factor_bps < 10_000,
        "invalid market economics"
    );
    let curve = &params.interest_curve;
    require!(
        curve.base_bps <= 10_000
            && curve.slope_bps <= 10_000
            && curve.kink_slope_bps <= 10_000
            && curve.kink_utilization_bps <= 10_000,
        "invalid interest curve"
    );
    require!(params.min_borrow.0 > 0, "invalid market economics");
    require!(
        params.supply_cap.0 > 0 && params.borrow_cap.0 > 0,
        "invalid caps"
    );
    require!(params.supply_cap.0 >= params.min_deposit.0, "invalid caps");
    require!(params.borrow_cap.0 >= params.min_borrow.0, "invalid caps");
    require!(
        params.collateral_price_feed != params.loan_price_feed,
        "duplicate price feed"
    );
}

impl ParamUpdate {
    fn apply_to(&self, params: &mut MarketParams) {
        match self {
            ParamUpdate::CollateralFactorBps(value) => params.collateral_factor_bps = *value,
            ParamUpdate::LiquidationThresholdBps(value) => {
                params.liquidation_threshold_bps = *value
            }
            ParamUpdate::LiquidationPenaltyBps(value) => params.liquidation_penalty_bps = *value,
            ParamUpdate::ReserveFactorBps(value) => params.reserve_factor_bps = *value,
            ParamUpdate::SupplyCap(value) => params.supply_cap = *value,
            ParamUpdate::BorrowCap(value) => params.borrow_cap = *value,
            ParamUpdate::MinBorrow(value) => params.min_borrow = *value,
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
            next_market_id: 1,
            markets: LookupMap::new(StorageKey::Markets),
            market_keys: LookupSet::new(StorageKey::MarketKeys),
            pending: LookupMap::new(StorageKey::PendingParamChanges),
        }
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    pub fn get_timelock_secs(&self) -> u64 {
        self.timelock_secs
    }

    pub fn get_market_count(&self) -> u32 {
        self.next_market_id - 1
    }

    pub fn get_market(&self, market_id: u32) -> MarketEntry {
        self.markets
            .get(&market_id)
            .expect("market not found")
            .clone()
    }

    pub fn get_pending_param_change(&self, market_id: u32) -> Option<PendingParamChange> {
        self.pending.get(&market_id).cloned()
    }

    pub fn register_market(&mut self, params: MarketParams) -> u32 {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        validate_market_params(&params);
        let key = MarketKey {
            chain_id: params.chain_id,
            loan_asset: params.loan_asset.account.clone(),
        };
        require!(self.market_keys.insert(key), "market already exists");
        let market_id = self.next_market_id;
        self.markets.insert(
            market_id,
            MarketEntry {
                params,
                state: PoolState {
                    total_supply: U128(0),
                    total_borrow: U128(0),
                    total_reserves: U128(0),
                    interest_index: U128(INITIAL_INTEREST_INDEX),
                    last_accrual_secs: now_secs(),
                },
            },
        );
        self.next_market_id += 1;
        env::log_str(
            &json!({
                "event": "market_registered",
                "market_id": market_id,
                "chain_id": self.markets.get(&market_id).expect("market").params.chain_id,
                "loan_asset": self.markets.get(&market_id).expect("market").params.loan_asset.account,
            })
            .to_string(),
        );
        market_id
    }

    pub fn propose_param_update(&mut self, market_id: u32, update: ParamUpdate) {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        require!(
            self.pending.get(&market_id).is_none(),
            "pending change exists"
        );
        let mut market = self
            .markets
            .get(&market_id)
            .expect("market not found")
            .clone();
        update.apply_to(&mut market.params);
        validate_market_params(&market.params);
        self.pending.insert(
            market_id,
            PendingParamChange {
                update,
                execute_at_secs: now_secs() + self.timelock_secs,
            },
        );
        env::log_str(
            &json!({
                "event": "param_update_proposed",
                "market_id": market_id,
                "execute_at_secs": now_secs() + self.timelock_secs,
            })
            .to_string(),
        );
    }

    pub fn cancel_param_update(&mut self, market_id: u32) {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        require!(self.pending.get(&market_id).is_some(), "no pending change");
        self.pending.remove(&market_id);
        env::log_str(
            &json!({
                "event": "param_update_cancelled",
                "market_id": market_id,
            })
            .to_string(),
        );
    }

    pub fn confirm_param_update(&mut self, market_id: u32) {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        let pending = self
            .pending
            .get(&market_id)
            .expect("no pending change")
            .clone();
        require!(
            now_secs() >= pending.execute_at_secs,
            "timelock not elapsed"
        );
        let mut market = self
            .markets
            .get(&market_id)
            .expect("market not found")
            .clone();
        pending.update.apply_to(&mut market.params);
        validate_market_params(&market.params);
        self.markets.insert(market_id, market);
        self.pending.remove(&market_id);
        env::log_str(
            &json!({
                "event": "param_update_applied",
                "market_id": market_id,
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

    fn market_params() -> MarketParams {
        MarketParams {
            chain_family: ChainFamily::Evm,
            chain_id: 11_155_111,
            light_client: "client-eth2.sepolia.testnet".parse().unwrap(),
            collateral_asset: Some(Asset {
                kind: AssetKind::NativeEth,
                account: "native".to_string(),
                decimals: 18,
            }),
            loan_asset: Asset {
                kind: AssetKind::Nep141,
                account: "usdc.testnet".to_string(),
                decimals: 6,
            },
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
            collateral_price_feed: "feed-eth.testnet".parse().unwrap(),
            loan_price_feed: "feed-usdc.testnet".parse().unwrap(),
        }
    }

    fn owner_market() -> Market {
        testing_env!(context("owner.testnet", 1_000).build());
        Market::new("owner.testnet".parse().unwrap(), 300)
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

    fn owner_market_with(timelock_secs: u64) -> Market {
        testing_env!(context("owner.testnet", 1_000).build());
        Market::new("owner.testnet".parse().unwrap(), timelock_secs)
    }

    #[test]
    fn register_market_assigns_sequential_ids() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let first = market.register_market(market_params());
        let mut other = market_params();
        other.chain_id = 84532;
        other.light_client = "client-eth2.base-sepolia.testnet".parse().unwrap();
        let second = market.register_market(other);
        assert_eq!(first, 1);
        assert_eq!(second, 2);
        assert_eq!(market.get_market_count(), 2);
        let stored = market.get_market(1);
        assert_eq!(stored.params.chain_id, 11_155_111);
        assert_eq!(stored.state.interest_index.0, INITIAL_INTEREST_INDEX);
        assert_eq!(stored.state.total_supply.0, 0);
    }

    #[test]
    fn register_market_rejects_duplicate_loan_asset_on_chain() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_market(market_params());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_market(market_params());
        }));
        assert!(result.is_err());
    }

    #[test]
    fn register_market_allows_same_asset_on_different_chain() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_market(market_params());
        let mut other = market_params();
        other.chain_id = 84532;
        other.light_client = "client-eth2.base-sepolia.testnet".parse().unwrap();
        let id = market.register_market(other);
        assert_eq!(id, 2);
    }

    #[test]
    fn non_owner_cannot_register_market() {
        let mut market = owner_market();
        testing_env!(context("attacker.testnet", 1_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_market(market_params());
        }));
        assert!(result.is_err());
        assert_eq!(market.get_market_count(), 0);
    }

    #[test]
    fn register_market_rejects_inverted_economics() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let mut params = market_params();
        params.liquidation_threshold_bps = 7_000;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_market(params);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn register_market_rejects_bad_loan_asset() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let mut params = market_params();
        params.loan_asset.account = "not an account".to_string();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_market(params);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn register_market_rejects_missing_collateral() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let mut params = market_params();
        params.collateral_asset = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_market(params);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn propose_rejects_change_that_breaks_economics() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_market(market_params());
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
        market.register_market(market_params());
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
        assert_eq!(market.get_market(1).params.reserve_factor_bps, 2_000);
    }

    #[test]
    fn cancel_clears_pending_change() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_market(market_params());
        testing_env!(context("owner.testnet", 2_000).build());
        market.propose_param_update(1, ParamUpdate::MinBorrow(U128(2_000_000)));
        market.cancel_param_update(1);
        assert!(market.get_pending_param_change(1).is_none());
        testing_env!(context("owner.testnet", 3_000).build());
        let confirmed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.confirm_param_update(1);
        }));
        assert!(confirmed.is_err());
        assert_eq!(market.get_market(1).params.min_borrow.0, 1_000_000);
    }

    #[test]
    fn confirm_requires_pending_change() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_market(market_params());
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
        market.register_market(market_params());
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
