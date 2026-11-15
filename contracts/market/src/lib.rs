use ethnum::U256;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::env;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::store::{IterableMap, LookupMap, LookupSet};
use near_sdk::{
    near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise,
    PromiseOrValue,
};

pub const MIN_TIMELOCK_SECS: u64 = 60;
pub const INDEX_SCALE: u128 = 10u128.pow(18);
const SECONDS_PER_YEAR: u64 = 31_536_000;
const ONE_YOCTO: u128 = 1;
const GAS_FT_TRANSFER: u64 = 10_000_000_000_000;
const GAS_WITHDRAWAL_CALLBACK: u64 = 10_000_000_000_000;

#[derive(BorshStorageKey)]
#[near(serializers = [borsh])]
enum StorageKey {
    Assets,
    AssetKeys,
    PendingParamChanges,
    SupplierShares,
    OwedWithdrawals,
    WithdrawalQueue,
    TokenToAsset,
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

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct QueuedWithdrawal {
    pub requested: U128,
    pub reserved_shares: u128,
    pub enqueued_at_secs: u64,
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
    supplier_shares: LookupMap<(u32, AccountId), u128>,
    owed: LookupMap<(u32, AccountId), u128>,
    queue: IterableMap<(u32, AccountId), QueuedWithdrawal>,
    token_to_asset: LookupMap<AccountId, u32>,
}

fn u256_to_u128(value: U256) -> u128 {
    require!(value <= U256::from(u128::MAX), "arithmetic overflow");
    value.0[0]
}

fn mul_div(a: u128, b: u128, denominator: u128) -> u128 {
    u256_to_u128(U256::from(a) * U256::from(b) / U256::from(denominator))
}

fn mul_div_ceil(a: u128, b: u128, denominator: u128) -> u128 {
    u256_to_u128(
        (U256::from(a) * U256::from(b) + U256::from(denominator - 1)) / U256::from(denominator),
    )
}

fn mul_div3(a: u128, b: u128, c: u128, denominator: u128) -> u128 {
    u256_to_u128(U256::from(a) * U256::from(b) * U256::from(c) / U256::from(denominator))
}

fn now_secs() -> u64 {
    env::block_timestamp() / 1_000_000_000
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

fn utilization_bps(pool: &AssetPoolState) -> u128 {
    if pool.total_supply.0 == 0 {
        return 0;
    }
    mul_div(pool.total_borrow.0, 10_000, pool.total_supply.0).min(10_000)
}

fn rate_bps(curve: &InterestCurve, util: u128) -> u128 {
    let kink = curve.kink_utilization_bps as u128;
    if util <= kink {
        if kink == 0 {
            curve.base_bps as u128
        } else {
            curve.base_bps as u128 + mul_div(curve.slope_bps as u128, util, kink)
        }
    } else {
        let above = util - kink;
        let span = (10_000 - kink).max(1);
        curve.base_bps as u128
            + curve.slope_bps as u128
            + mul_div(curve.kink_slope_bps as u128, above, span)
    }
}

fn accrue(pool: &mut AssetPoolState, config: &AssetConfig) {
    let now = now_secs();
    let elapsed = now.saturating_sub(pool.last_accrual_secs);
    if pool.total_borrow.0 == 0 || elapsed == 0 {
        pool.last_accrual_secs = now;
        return;
    }
    let util = utilization_bps(pool);
    let rate = rate_bps(&config.interest_curve, util);
    let interest = mul_div3(
        pool.total_borrow.0,
        rate,
        elapsed as u128,
        10_000u128 * SECONDS_PER_YEAR as u128,
    );
    if interest == 0 {
        pool.last_accrual_secs = now;
        return;
    }
    let reserve_cut = mul_div(interest, config.reserve_factor_bps as u128, 10_000);
    let supplier_cut = interest - reserve_cut;
    if pool.total_supply.0 > 0 && supplier_cut > 0 {
        let delta = mul_div(supplier_cut, INDEX_SCALE, pool.total_supply.0);
        let new_index = pool
            .interest_index
            .0
            .checked_add(delta)
            .expect("index overflow");
        pool.interest_index = U128(new_index);
    }
    pool.total_borrow = U128(pool.total_borrow.0 + interest);
    pool.total_reserves = U128(pool.total_reserves.0 + reserve_cut);
    pool.last_accrual_secs = now;
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
            supplier_shares: LookupMap::new(StorageKey::SupplierShares),
            owed: LookupMap::new(StorageKey::OwedWithdrawals),
            queue: IterableMap::new(StorageKey::WithdrawalQueue),
            token_to_asset: LookupMap::new(StorageKey::TokenToAsset),
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

    pub fn get_supplier_shares(&self, asset_id: u32, account: AccountId) -> U128 {
        U128(
            self.supplier_shares
                .get(&(asset_id, account.clone()))
                .copied()
                .unwrap_or(0),
        )
    }

    pub fn get_owed_withdrawal(&self, asset_id: u32, account: AccountId) -> U128 {
        U128(
            self.owed
                .get(&(asset_id, account.clone()))
                .copied()
                .unwrap_or(0),
        )
    }

    pub fn get_queued_withdrawal(
        &self,
        asset_id: u32,
        account: AccountId,
    ) -> Option<QueuedWithdrawal> {
        self.queue.get(&(asset_id, account.clone())).cloned()
    }

    pub fn get_liquidity(&self, asset_id: u32) -> U128 {
        let entry = self.assets.get(&asset_id).expect("asset not found");
        U128(
            entry
                .pool
                .total_supply
                .0
                .saturating_sub(entry.pool.total_borrow.0),
        )
    }

    pub fn get_withdrawable_amount(&self, asset_id: u32, account: AccountId) -> U128 {
        let balance = self
            .supplier_shares
            .get(&(asset_id, account.clone()))
            .copied()
            .unwrap_or(0);
        let index = self
            .assets
            .get(&asset_id)
            .expect("asset not found")
            .pool
            .interest_index
            .0;
        U128(mul_div(balance, index, INDEX_SCALE))
    }

    fn entry_of(&mut self, asset_id: u32) -> AssetEntry {
        self.assets.get(&asset_id).expect("asset not found").clone()
    }

    fn balance_or_insert(&mut self, asset_id: u32, account: &AccountId) -> &mut u128 {
        if !self
            .supplier_shares
            .contains_key(&(asset_id, account.clone()))
        {
            self.supplier_shares.insert((asset_id, account.clone()), 0);
        }
        self.supplier_shares
            .get_mut(&(asset_id, account.clone()))
            .unwrap()
    }

    fn burn_shares(&mut self, asset_id: u32, account: &AccountId, shares: u128) {
        let balance = self
            .supplier_shares
            .get(&(asset_id, account.clone()))
            .copied()
            .unwrap_or(0);
        require!(balance >= shares, "insufficient shares");
        if balance == shares {
            self.supplier_shares.remove(&(asset_id, account.clone()));
        } else {
            *self
                .supplier_shares
                .get_mut(&(asset_id, account.clone()))
                .unwrap() = balance - shares;
        }
    }

    fn deposit_internal(&mut self, asset_id: u32, sender: AccountId, amount: u128) {
        require!(amount > 0, "zero deposit");
        let mut entry = self.entry_of(asset_id);
        require!(entry.config.supply_enabled, "asset is not a supply asset");
        accrue(&mut entry.pool, &entry.config);
        require!(
            amount >= entry.config.min_deposit.0,
            "deposit below minimum"
        );
        require!(
            entry
                .pool
                .total_supply
                .0
                .checked_add(amount)
                .is_some_and(|total| total <= entry.config.supply_cap.0),
            "supply cap reached"
        );
        let shares = mul_div(amount, INDEX_SCALE, entry.pool.interest_index.0);
        *self.balance_or_insert(asset_id, &sender) += shares;
        entry.pool.total_supply = U128(entry.pool.total_supply.0 + amount);
        self.assets.insert(asset_id, entry);
        env::log_str(
            &json!({
                "event": "supplied",
                "asset_id": asset_id,
                "account": sender,
                "amount": amount.to_string(),
                "shares": shares.to_string(),
            })
            .to_string(),
        );
    }

    fn send_asset(&mut self, asset_id: u32, account: &AccountId, amount: u128) {
        let config = self
            .assets
            .get(&asset_id)
            .expect("asset not found")
            .config
            .clone();
        let token: AccountId = config.asset.account.parse().unwrap();
        let args = json!({"receiver_id": account, "amount": amount.to_string()})
            .to_string()
            .into_bytes();
        let _ = Promise::new(token)
            .function_call(
                "ft_transfer".to_string(),
                args,
                NearToken::from_yoctonear(ONE_YOCTO),
                Gas::from_gas(GAS_FT_TRANSFER),
            )
            .then(
                Promise::new(env::current_account_id()).function_call(
                    "on_withdrawal_result".to_string(),
                    json!({
                        "asset_id": asset_id,
                        "account": account,
                        "amount": amount.to_string(),
                    })
                    .to_string()
                    .into_bytes(),
                    NearToken::from_yoctonear(0),
                    Gas::from_gas(GAS_WITHDRAWAL_CALLBACK),
                ),
            );
    }

    #[payable]
    pub fn withdraw(&mut self, asset_id: u32, amount: U128) {
        require!(amount.0 > 0, "zero withdraw");
        let account = env::predecessor_account_id();
        let mut entry = self.entry_of(asset_id);
        accrue(&mut entry.pool, &entry.config);
        let shares_out = mul_div_ceil(amount.0, INDEX_SCALE, entry.pool.interest_index.0);
        let balance = self
            .supplier_shares
            .get(&(asset_id, account.clone()))
            .copied()
            .unwrap_or(0);
        require!(balance >= shares_out, "insufficient shares");
        require!(
            amount.0
                <= entry
                    .pool
                    .total_supply
                    .0
                    .saturating_sub(entry.pool.total_borrow.0),
            "insufficient liquidity, queue withdrawal instead"
        );
        self.burn_shares(asset_id, &account, shares_out);
        entry.pool.total_supply = U128(entry.pool.total_supply.0 - amount.0);
        let owed_key = (asset_id, account.clone());
        let owed_now = self.owed.get(&owed_key).copied().unwrap_or(0) + amount.0;
        self.owed.insert(owed_key, owed_now);
        self.assets.insert(asset_id, entry);
        self.send_asset(asset_id, &account, amount.0);
        env::log_str(
            &json!({
                "event": "withdrawn",
                "asset_id": asset_id,
                "account": account,
                "amount": amount.0.to_string(),
                "shares": shares_out.to_string(),
            })
            .to_string(),
        );
    }

    #[payable]
    pub fn queue_withdrawal(&mut self, asset_id: u32, amount: U128) {
        require!(amount.0 > 0, "zero withdraw");
        let account = env::predecessor_account_id();
        let mut entry = self.entry_of(asset_id);
        require!(entry.config.supply_enabled, "asset is not a supply asset");
        accrue(&mut entry.pool, &entry.config);
        require!(
            amount.0 >= entry.config.min_deposit.0,
            "deposit below minimum"
        );
        let shares_out = mul_div_ceil(amount.0, INDEX_SCALE, entry.pool.interest_index.0);
        let balance = self
            .supplier_shares
            .get(&(asset_id, account.clone()))
            .copied()
            .unwrap_or(0);
        require!(balance >= shares_out, "insufficient shares");
        self.burn_shares(asset_id, &account, shares_out);
        let key = (asset_id, account.clone());
        let mut queued = self.queue.get(&key).cloned().unwrap_or(QueuedWithdrawal {
            requested: U128(0),
            reserved_shares: 0,
            enqueued_at_secs: now_secs(),
        });
        queued.requested = U128(queued.requested.0 + amount.0);
        queued.reserved_shares += shares_out;
        self.queue.insert(key, queued);
        self.assets.insert(asset_id, entry);
        env::log_str(
            &json!({
                "event": "withdraw_queued",
                "asset_id": asset_id,
                "account": account,
                "amount": amount.0.to_string(),
                "shares": shares_out.to_string(),
            })
            .to_string(),
        );
    }

    #[payable]
    pub fn cancel_queued_withdrawal(&mut self, asset_id: u32) {
        let account = env::predecessor_account_id();
        let key = (asset_id, account.clone());
        let queued = self.queue.get(&key).cloned().expect("no queued withdrawal");
        *self.balance_or_insert(asset_id, &account) += queued.reserved_shares;
        self.queue.remove(&key);
        env::log_str(
            &json!({
                "event": "withdraw_cancelled",
                "asset_id": asset_id,
                "account": account,
                "shares": queued.reserved_shares.to_string(),
            })
            .to_string(),
        );
    }

    #[payable]
    pub fn claim_withdrawal(&mut self, asset_id: u32) {
        let account = env::predecessor_account_id();
        let owed_key = (asset_id, account.clone());
        let mut entry = self.entry_of(asset_id);
        accrue(&mut entry.pool, &entry.config);
        let owed_amount = self.owed.get(&owed_key).copied().unwrap_or(0);
        let mut payable = owed_amount;
        let mut queue_pay = 0u128;
        if let Some(queued) = self.queue.get(&owed_key).cloned() {
            queue_pay = mul_div(
                queued.reserved_shares,
                entry.pool.interest_index.0,
                INDEX_SCALE,
            );
            payable += queue_pay;
        }
        require!(payable > 0, "nothing to claim");
        require!(
            payable
                <= entry
                    .pool
                    .total_supply
                    .0
                    .saturating_sub(entry.pool.total_borrow.0),
            "insufficient liquidity"
        );
        if queue_pay > 0 {
            self.queue.remove(&owed_key);
            entry.pool.total_supply = U128(entry.pool.total_supply.0 - queue_pay);
        }
        self.owed.insert(owed_key, payable);
        self.assets.insert(asset_id, entry);
        self.send_asset(asset_id, &account, payable);
        env::log_str(
            &json!({
                "event": "withdraw_claimed",
                "asset_id": asset_id,
                "account": account,
                "amount": payable.to_string(),
                "owed": owed_amount.to_string(),
            })
            .to_string(),
        );
    }

    pub fn accrue_asset(&mut self, asset_id: u32) {
        let mut entry = self.entry_of(asset_id);
        accrue(&mut entry.pool, &entry.config);
        self.assets.insert(asset_id, entry);
        env::log_str(
            &json!({
                "event": "interest_accrued",
                "asset_id": asset_id,
            })
            .to_string(),
        );
    }

    pub fn on_withdrawal_result(&mut self, asset_id: u32, account: AccountId, amount: U128) {
        require!(
            env::predecessor_account_id() == env::current_account_id(),
            "internal only"
        );
        let owed_key = (asset_id, account);
        if env::promise_result_checked(0, 1024).is_ok() {
            let remaining = self
                .owed
                .get(&owed_key)
                .copied()
                .unwrap_or(0)
                .saturating_sub(amount.0);
            if remaining == 0 {
                self.owed.remove(&owed_key);
            } else {
                self.owed.insert(owed_key, remaining);
            }
            env::log_str(
                &json!({
                    "event": "withdrawal_delivered",
                    "asset_id": asset_id,
                    "amount": amount.0.to_string(),
                })
                .to_string(),
            );
        } else {
            env::log_str(
                &json!({
                    "event": "withdrawal_failed_retry_queued",
                    "asset_id": asset_id,
                    "amount": amount.0.to_string(),
                })
                .to_string(),
            );
        }
    }

    pub fn register_asset(&mut self, config: AssetConfig) -> u32 {
        require!(env::predecessor_account_id() == self.owner, "owner only");
        validate_asset_config(&config);
        let key = AssetKey {
            chain_id: match config.asset.kind {
                AssetKind::Nep141 => 0,
                AssetKind::NativeEth => config.chain_id,
            },
            account: config.asset.account.clone(),
        };
        require!(self.asset_keys.insert(key), "asset already exists");
        let asset_id = self.next_asset_id;
        let chain_id = config.chain_id;
        let account = config.asset.account.clone();
        if let AssetKind::Nep141 = config.asset.kind {
            self.token_to_asset
                .insert(account.parse().unwrap(), asset_id);
        }
        self.assets.insert(
            asset_id,
            AssetEntry {
                config,
                pool: AssetPoolState {
                    total_supply: U128(0),
                    total_borrow: U128(0),
                    total_reserves: U128(0),
                    interest_index: U128(INDEX_SCALE),
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
        let mut entry = self.entry_of(asset_id);
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
        let mut entry = self.entry_of(asset_id);
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

#[near]
impl FungibleTokenReceiver for Market {
    #[payable]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        require!(msg.is_empty(), "unexpected message");
        let asset_id = self
            .token_to_asset
            .get(&env::predecessor_account_id())
            .copied()
            .expect("unknown token");
        self.deposit_internal(asset_id, sender_id, amount.0);
        PromiseOrValue::Value(U128(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;
    use proptest::prelude::*;

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
            min_deposit: U128(1_000),
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
            min_deposit: U128(1_000),
            min_borrow: U128(1_000_000),
            supply_cap: U128(u128::MAX),
            borrow_cap: U128(u128::MAX),
            price_feed: "feed-eth.testnet".parse().unwrap(),
        }
    }

    fn reset_storage() {
        near_sdk::mock::with_mocked_blockchain(|b| {
            b.take_storage();
        });
    }

    fn owner_market() -> Market {
        reset_storage();
        testing_env!(context("owner.testnet", 1_000).build());
        Market::new("owner.testnet".parse().unwrap(), 300)
    }

    fn register_usdc(market: &mut Market) -> u32 {
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_asset(usdc_config())
    }

    fn deposit_as(market: &mut Market, depositor: &str, asset_id: u32, amount: u128) {
        market.deposit_internal(asset_id, depositor.parse().unwrap(), amount);
    }

    fn set_borrow(market: &mut Market, asset_id: u32, amount: u128) {
        let mut entry = market.assets.get(&asset_id).unwrap().clone();
        entry.pool.total_borrow = U128(amount);
        market.assets.insert(asset_id, entry);
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
        let market = Market::new("owner.testnet".parse().unwrap(), 60);
        assert_eq!(market.get_timelock_secs(), 60);
    }

    #[test]
    fn register_asset_assigns_sequential_ids() {
        let mut market = owner_market();
        let usdc = register_usdc(&mut market);
        testing_env!(context("owner.testnet", 1_000).build());
        let usdt = market.register_asset(usdt_config());
        assert_eq!(usdc, 1);
        assert_eq!(usdt, 2);
        assert_eq!(market.get_asset_count(), 2);
        let stored = market.get_asset(1);
        assert_eq!(stored.config.asset.account, "usdc.testnet");
        assert!(stored.config.supply_enabled);
        assert!(!stored.config.collateral_enabled);
        assert_eq!(stored.pool.interest_index.0, INDEX_SCALE);
        assert_eq!(stored.pool.total_supply.0, 0);
    }

    #[test]
    fn register_asset_rejects_duplicate_on_chain() {
        let mut market = owner_market();
        register_usdc(&mut market);
        testing_env!(context("owner.testnet", 1_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_asset(usdc_config());
        }));
        assert!(result.is_err());
    }

    #[test]
    fn register_asset_allows_native_eth_on_different_chains() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        let eth_sepolia = market.register_asset(eth_config());
        let mut eth_base = eth_config();
        eth_base.chain_id = 84532;
        eth_base.light_client = "client-eth2.base-sepolia.testnet".parse().unwrap();
        eth_base.price_feed = "feed-eth-base.testnet".parse().unwrap();
        let eth_base_id = market.register_asset(eth_base);
        assert_eq!(eth_sepolia, 1);
        assert_eq!(eth_base_id, 2);
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
    fn same_nep141_token_cannot_register_twice_on_any_chain() {
        let mut market = owner_market();
        register_usdc(&mut market);
        testing_env!(context("owner.testnet", 1_000).build());
        let mut base_usdc = usdc_config();
        base_usdc.chain_id = 84532;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.register_asset(base_usdc);
        }));
        assert!(result.is_err());
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
        register_usdc(&mut market);
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
        register_usdc(&mut market);
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
        register_usdc(&mut market);
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
        register_usdc(&mut market);
        testing_env!(context("owner.testnet", 5_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.confirm_param_update(1);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn non_owner_cannot_propose_or_confirm() {
        let mut market = owner_market();
        register_usdc(&mut market);
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

    #[test]
    fn deposit_mints_shares_proportionally() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 1_000_000);
        deposit_as(&mut market, "bob.testnet", 1, 3_000_000);
        assert_eq!(
            market
                .get_supplier_shares(1, "alice.testnet".parse().unwrap())
                .0,
            1_000_000
        );
        assert_eq!(
            market
                .get_supplier_shares(1, "bob.testnet".parse().unwrap())
                .0,
            3_000_000
        );
        assert_eq!(market.get_asset(1).pool.total_supply.0, 4_000_000);
    }

    #[test]
    fn deposit_respects_minimum_and_cap() {
        let mut market = owner_market();
        register_usdc(&mut market);
        let tiny = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            deposit_as(&mut market, "alice.testnet", 1, 999);
        }));
        assert!(tiny.is_err());
        let mut entry = market.assets.get(&1).unwrap().clone();
        entry.config.supply_cap = U128(5_000_000);
        market.assets.insert(1, entry);
        deposit_as(&mut market, "alice.testnet", 1, 4_000_000);
        let capped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            deposit_as(&mut market, "bob.testnet", 1, 2_000_000);
        }));
        assert!(capped.is_err());
        assert_eq!(market.get_asset(1).pool.total_supply.0, 4_000_000);
    }

    #[test]
    fn deposit_rejects_non_supply_asset() {
        let mut market = owner_market();
        testing_env!(context("owner.testnet", 1_000).build());
        market.register_asset(eth_config());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            deposit_as(&mut market, "alice.testnet", 1, 1_000);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn ft_on_transfer_routes_by_token_and_rejects_unknown() {
        let mut market = owner_market();
        register_usdc(&mut market);
        testing_env!(context("usdc.testnet", 2_000).build());
        let _ = market.ft_on_transfer(
            "alice.testnet".parse().unwrap(),
            U128(500_000),
            String::new(),
        );
        assert_eq!(
            market
                .get_supplier_shares(1, "alice.testnet".parse().unwrap())
                .0,
            500_000
        );
        testing_env!(context("unknown-token.testnet", 2_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = market.ft_on_transfer(
                "alice.testnet".parse().unwrap(),
                U128(500_000),
                String::new(),
            );
        }));
        assert!(result.is_err());
    }

    #[test]
    fn accrual_splits_interest_between_suppliers_and_reserves() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 1_000_000_000);
        set_borrow(&mut market, 1, 500_000_000);
        let index_before = market.get_asset(1).pool.interest_index.0;
        let reserves_before = market.get_asset(1).pool.total_reserves.0;

        testing_env!(context("keeper.testnet", 1_000 + SECONDS_PER_YEAR).build());
        market.accrue_asset(1);
        let entry = market.get_asset(1);
        assert!(entry.pool.interest_index.0 > index_before);
        assert!(entry.pool.total_reserves.0 > reserves_before);
        assert!(entry.pool.total_borrow.0 > 500_000_000);

        let rate_at_50pct = rate_bps(&entry.config.interest_curve, 5_000);
        let interest = mul_div3(
            500_000_000,
            rate_at_50pct,
            SECONDS_PER_YEAR as u128,
            10_000u128 * SECONDS_PER_YEAR as u128,
        );
        let reserve_cut = mul_div(interest, 1_000, 10_000);
        let supplier_cut = interest - reserve_cut;
        assert_eq!(entry.pool.total_reserves.0, reserves_before + reserve_cut);
        assert_eq!(entry.pool.total_borrow.0, 500_000_000 + interest);
        let expected_index = index_before + mul_div(supplier_cut, INDEX_SCALE, 1_000_000_000);
        assert_eq!(entry.pool.interest_index.0, expected_index);
    }

    #[test]
    fn accrual_skips_when_no_borrow_or_no_time() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 1_000_000);
        testing_env!(context("keeper.testnet", 5_000).build());
        market.accrue_asset(1);
        assert_eq!(market.get_asset(1).pool.interest_index.0, INDEX_SCALE);
        assert_eq!(market.get_asset(1).pool.last_accrual_secs, 5_000);
        set_borrow(&mut market, 1, 500_000);
        market.accrue_asset(1);
        assert_eq!(market.get_asset(1).pool.interest_index.0, INDEX_SCALE);
    }

    #[test]
    fn withdraw_burns_shares_and_supply() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 2_000_000);
        testing_env!(context("alice.testnet", 2_000).build());
        market.withdraw(1, U128(500_000));
        assert_eq!(
            market
                .get_supplier_shares(1, "alice.testnet".parse().unwrap())
                .0,
            1_500_000
        );
        assert_eq!(market.get_asset(1).pool.total_supply.0, 1_500_000);
        assert_eq!(
            market
                .get_owed_withdrawal(1, "alice.testnet".parse().unwrap())
                .0,
            500_000
        );
    }

    #[test]
    fn withdraw_never_exceeds_share_value() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 2_000_000);
        testing_env!(context("alice.testnet", 2_000).build());
        let index = market.get_asset(1).pool.interest_index.0;
        let shares = market
            .get_supplier_shares(1, "alice.testnet".parse().unwrap())
            .0;
        let max_amount = mul_div(shares, index, INDEX_SCALE);
        let over = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.withdraw(1, U128(max_amount + 1));
        }));
        assert!(over.is_err());
        assert_eq!(
            market
                .get_supplier_shares(1, "alice.testnet".parse().unwrap())
                .0,
            shares
        );
        market.withdraw(1, U128(max_amount));
        assert_eq!(
            market
                .get_supplier_shares(1, "alice.testnet".parse().unwrap())
                .0,
            shares - mul_div_ceil(max_amount, INDEX_SCALE, index)
        );
    }

    #[test]
    fn withdraw_requires_liquidity() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 2_000_000);
        set_borrow(&mut market, 1, 1_800_000);
        testing_env!(context("alice.testnet", 2_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.withdraw(1, U128(500_000));
        }));
        assert!(result.is_err());
        assert_eq!(
            market
                .get_supplier_shares(1, "alice.testnet".parse().unwrap())
                .0,
            2_000_000
        );
        assert_eq!(market.get_asset(1).pool.total_supply.0, 2_000_000);
    }

    #[test]
    fn queue_then_claim_pays_full_value() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 2_000_000);
        set_borrow(&mut market, 1, 1_900_000);
        testing_env!(context("alice.testnet", 2_000).build());
        market.queue_withdrawal(1, U128(800_000));
        let queued = market
            .get_queued_withdrawal(1, "alice.testnet".parse().unwrap())
            .expect("queued");
        assert_eq!(queued.requested.0, 800_000);
        assert!(queued.reserved_shares > 0);
        assert_eq!(
            market
                .get_supplier_shares(1, "alice.testnet".parse().unwrap())
                .0,
            2_000_000 - queued.reserved_shares
        );

        set_borrow(&mut market, 1, 0);
        testing_env!(context("alice.testnet", 3_000).build());
        market.claim_withdrawal(1);
        assert!(market
            .get_queued_withdrawal(1, "alice.testnet".parse().unwrap())
            .is_none());
        assert!(
            market
                .get_owed_withdrawal(1, "alice.testnet".parse().unwrap())
                .0
                >= 800_000
        );
    }

    #[test]
    fn cancel_queue_restores_shares() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 2_000_000);
        testing_env!(context("alice.testnet", 2_000).build());
        market.queue_withdrawal(1, U128(800_000));
        let reserved = market
            .get_queued_withdrawal(1, "alice.testnet".parse().unwrap())
            .expect("queued")
            .reserved_shares;
        market.cancel_queued_withdrawal(1);
        assert!(market
            .get_queued_withdrawal(1, "alice.testnet".parse().unwrap())
            .is_none());
        assert_eq!(
            market
                .get_supplier_shares(1, "alice.testnet".parse().unwrap())
                .0,
            2_000_000
        );
        assert_eq!(reserved, 800_000);
    }

    #[test]
    fn claim_requires_liquidity_and_keeps_queue() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 2_000_000);
        set_borrow(&mut market, 1, 1_900_000);
        testing_env!(context("alice.testnet", 2_000).build());
        market.queue_withdrawal(1, U128(800_000));
        let illiquid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.claim_withdrawal(1);
        }));
        assert!(illiquid.is_err());
        assert!(market
            .get_queued_withdrawal(1, "alice.testnet".parse().unwrap())
            .is_some());
        testing_env!(context("alice.testnet", 3_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.claim_withdrawal(1);
        }));
        assert!(result.is_err());
        assert!(market
            .get_queued_withdrawal(1, "alice.testnet".parse().unwrap())
            .is_some());
    }

    #[test]
    fn claim_with_nothing_panics() {
        let mut market = owner_market();
        register_usdc(&mut market);
        testing_env!(context("alice.testnet", 2_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.claim_withdrawal(1);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn owed_retry_replays_without_double_burn() {
        let mut market = owner_market();
        register_usdc(&mut market);
        deposit_as(&mut market, "alice.testnet", 1, 2_000_000);
        set_borrow(&mut market, 1, 1_000_000);
        testing_env!(context("alice.testnet", 2_000).build());
        market.withdraw(1, U128(300_000));
        assert_eq!(
            market
                .get_owed_withdrawal(1, "alice.testnet".parse().unwrap())
                .0,
            300_000
        );
        market.claim_withdrawal(1);
        assert_eq!(
            market
                .get_owed_withdrawal(1, "alice.testnet".parse().unwrap())
                .0,
            300_000
        );
        set_borrow(&mut market, 1, 0);
        testing_env!(context("alice.testnet", 3_000).build());
        market.claim_withdrawal(1);
        assert!(
            market
                .get_owed_withdrawal(1, "alice.testnet".parse().unwrap())
                .0
                >= 300_000
        );
    }

    #[test]
    fn on_withdrawal_result_rejects_external_caller() {
        let mut market = owner_market();
        register_usdc(&mut market);
        testing_env!(context("attacker.testnet", 2_000).build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            market.on_withdrawal_result(1, "alice.testnet".parse().unwrap(), U128(100));
        }));
        assert!(result.is_err());
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]

        #[test]
        fn deposits_accumulate_shares_exactly(amounts in proptest::collection::vec(1_000u128..10_000_000_000, 1..24)) {
            let mut market = owner_market();
            register_usdc(&mut market);
            let mut expected_shares = 0u128;
            for (i, amount) in amounts.iter().enumerate() {
                deposit_as(&mut market, &format!("supplier{}.testnet", i), 1, *amount);
                expected_shares += *amount;
                prop_assert_eq!(
                    market.get_supplier_shares(1, format!("supplier{}.testnet", i).parse().unwrap()).0,
                    *amount
                );
                prop_assert_eq!(market.get_asset(1).pool.total_supply.0, expected_shares);
            }
        }

        #[test]
        fn accrual_is_monotonic_and_never_negative(
            borrows in proptest::collection::vec(0u128..1_000_000_000, 1..16),
            steps in proptest::collection::vec(1u64..40_000_000, 1..16),
        ) {
            let mut market = owner_market();
            register_usdc(&mut market);
            deposit_as(&mut market, "alice.testnet", 1, 1_000_000_000);
            let mut last_index = INDEX_SCALE;
            let mut last_reserves = 0u128;
            let mut time = 2_000u64;
            for (borrow, step) in borrows.iter().zip(steps.iter()) {
                set_borrow(&mut market, 1, *borrow);
                time += *step;
                testing_env!(context("keeper.testnet", time).build());
                market.accrue_asset(1);
                let entry = market.get_asset(1);
                prop_assert!(entry.pool.interest_index.0 >= last_index);
                prop_assert!(entry.pool.total_reserves.0 >= last_reserves);
                let supplier_value = mul_div(
                    market.get_supplier_shares(1, "alice.testnet".parse().unwrap()).0,
                    entry.pool.interest_index.0,
                    INDEX_SCALE,
                );
                prop_assert!(supplier_value >= 1_000_000_000);
                last_index = entry.pool.interest_index.0;
                last_reserves = entry.pool.total_reserves.0;
            }
        }

        #[test]
        fn rate_curve_is_monotonic_and_continuous_at_kink(utilis in proptest::collection::vec(0u128..10_000, 2..64)) {
            let mut market = owner_market();
            register_usdc(&mut market);
            let curve = market.get_asset(1).config.interest_curve.clone();
            let kink = curve.kink_utilization_bps as u128;
            let mut previous = 0u128;
            let mut sorted = utilis.clone();
            sorted.sort();
            for util in sorted {
                let rate = rate_bps(&curve, util);
                prop_assert!(rate >= previous);
                if util == kink {
                    let below = rate_bps(&curve, kink.saturating_sub(1));
                    prop_assert!(rate.saturating_sub(below) <= 1);
                }
                previous = rate;
            }
        }

        #[test]
        fn withdrawal_math_never_exceeds_owned_value(
            balance in 1u128..10_000_000_000_000,
            index in INDEX_SCALE..(INDEX_SCALE * 3),
            requested in 1u128..10_000_000_000_000,
        ) {
            let max_amount = mul_div(balance, index, INDEX_SCALE);
            if requested <= max_amount {
                let shares_out = mul_div_ceil(requested, INDEX_SCALE, index);
                prop_assert!(shares_out <= balance);
            }
        }
    }
}
