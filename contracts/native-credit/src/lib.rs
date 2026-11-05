pub mod crypto;
pub mod math;

use near_sdk::json_types::U128;
use near_sdk::serde::Deserialize;
use near_sdk::store::LookupMap;
use near_sdk::{
    env, near, require, AccountId, BorshStorageKey, CurveType, Gas, NearToken, PanicOnDefault,
    Promise, PromiseOrValue, PublicKey,
};
use serde_json::json;

const STORAGE_DEPOSIT_YOCTO: u128 = 20_000_000_000_000_000_000_000;
const ONE_YOCTO: u128 = 1;
const MAX_DERIVED_KEY_LEN: usize = 256;
const MAX_SIGN_RESPONSE_LEN: usize = 4_096;
const MAX_BLOCK_HASH_LEN: usize = 33;

#[derive(BorshStorageKey)]
#[near(serializers = [borsh])]
enum StorageKey {
    Positions,
    SepoliaAddresses,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Config {
    pub owner: AccountId,
    pub guardian: AccountId,
    pub usdc: AccountId,
    pub light_client: AccountId,
    pub mpc: AccountId,
    pub oracle_keys: Vec<PublicKey>,
    pub oracle_threshold: u8,
    pub collateral_factor_bps: u16,
    pub liquidation_threshold_bps: u16,
    pub liquidation_penalty_bps: u16,
    pub interest_rate_bps: u16,
    pub max_oracle_age_secs: u64,
    pub min_withdrawal_wei: U128,
    pub min_borrow_usdc: U128,
    pub risk_off: bool,
    pub chain_id: u64,
    pub gas_limit: u64,
    pub max_fee_per_gas: U128,
    pub max_priority_fee_per_gas: U128,
}

impl Config {
    fn max_gas_wei(&self) -> u128 {
        (self.gas_limit as u128)
            .checked_mul(self.max_fee_per_gas.0)
            .expect("gas overflow")
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Price {
    pub eth_usd_1e8: U128,
    pub usdc_usd_1e8: U128,
    pub timestamp: u64,
    pub sequence: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositionStatus {
    Deriving,
    Idle,
    WithdrawalPending,
    LiquidationPending,
    NeedsReview,
    Closed,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProofKind {
    Deposit,
    Withdrawal,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct PendingProof {
    pub header_hash: [u8; 32],
    pub header_number: u64,
    pub new_balance: U128,
    pub new_nonce: u64,
    pub kind: ProofKind,
    pub requested_block: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WithdrawalKind {
    User,
    Liquidation,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct PendingWithdrawal {
    pub amount_wei: U128,
    pub max_gas_wei: U128,
    pub nonce: u64,
    pub to: String,
    pub signing_hash: [u8; 32],
    pub raw_tx: Option<Vec<u8>>,
    pub tx_hash: Option<[u8; 32]>,
    pub kind: WithdrawalKind,
    pub requested_block: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TicketStatus {
    PendingSignature,
    Signed,
    Settled,
    Cancelled,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct LiquidationTicket {
    pub liquidator: AccountId,
    pub collateral_wei: U128,
    pub repay_usdc: U128,
    pub sepolia_address: String,
    pub status: TicketStatus,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Position {
    pub owner: AccountId,
    pub status: PositionStatus,
    pub deposit_address: Option<String>,
    pub collateral_wei: U128,
    pub last_verified_block: u64,
    pub last_verified_nonce: u64,
    pub debt_usdc: U128,
    pub last_accrual_secs: u64,
    pub pending_deposit: Option<PendingProof>,
    pub pending_withdrawal: Option<PendingWithdrawal>,
    pub pending_borrow: Option<U128>,
    pub liquidation_ticket: Option<LiquidationTicket>,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    pub config: Config,
    pub positions: LookupMap<u64, Position>,
    pub sepolia_addresses: LookupMap<AccountId, String>,
    pub next_position_id: u64,
    pub total_debt_usdc: u128,
    pub total_collateral_wei: u128,
    pub total_bad_debt_usdc: u128,
    pub price: Option<Price>,
}

#[near]
impl Contract {
    #[init]
    pub fn new(config: Config) -> Self {
        let mut config = config;
        require!(
            config.collateral_factor_bps > 0
                && config.collateral_factor_bps <= config.liquidation_threshold_bps
                && config.liquidation_threshold_bps <= 10_000,
            "invalid collateral parameters"
        );
        require!(
            config.liquidation_penalty_bps <= 5_000,
            "invalid liquidation penalty"
        );
        require!(
            config.max_oracle_age_secs > 0 && config.oracle_threshold > 0,
            "invalid oracle config"
        );
        require!(
            config.oracle_threshold as usize <= config.oracle_keys.len(),
            "oracle threshold too high"
        );
        for key in &config.oracle_keys {
            require!(
                key.curve_type() == CurveType::ED25519,
                "oracle key not ed25519"
            );
        }
        require!(
            config.gas_limit > 0
                && config.max_fee_per_gas.0 > 0
                && config.max_priority_fee_per_gas.0 <= config.max_fee_per_gas.0,
            "invalid gas config"
        );
        require!(config.chain_id > 0, "invalid chain id");
        require!(
            config.min_borrow_usdc.0 > 0 && config.min_withdrawal_wei.0 > config.max_gas_wei(),
            "invalid minimum amount"
        );

        config.owner = env::predecessor_account_id();
        Self {
            config,
            positions: LookupMap::new(StorageKey::Positions),
            sepolia_addresses: LookupMap::new(StorageKey::SepoliaAddresses),
            next_position_id: 1,
            total_debt_usdc: 0,
            total_collateral_wei: 0,
            total_bad_debt_usdc: 0,
            price: None,
        }
    }

    #[payable]
    pub fn create_position(&mut self) -> u64 {
        require!(
            env::attached_deposit() >= NearToken::from_yoctonear(STORAGE_DEPOSIT_YOCTO),
            "storage deposit required"
        );
        let id = self.next_position_id;
        self.next_position_id = self.next_position_id.checked_add(1).expect("position id");
        let position = Position {
            owner: env::predecessor_account_id(),
            status: PositionStatus::Deriving,
            deposit_address: None,
            collateral_wei: U128(0),
            last_verified_block: 0,
            last_verified_nonce: 0,
            debt_usdc: U128(0),
            last_accrual_secs: env::block_timestamp() / 1_000_000_000,
            pending_deposit: None,
            pending_withdrawal: None,
            pending_borrow: None,
            liquidation_ticket: None,
        };
        self.positions.insert(id, position);
        self.request_derived_key(id);
        id
    }

    pub fn transfer_position(&mut self, position_id: u64, new_owner: AccountId) {
        let mut position = self.positions.get(&position_id).cloned().expect("position");
        require!(
            position.owner == env::predecessor_account_id(),
            "owner only"
        );
        require!(position.status == PositionStatus::Idle, "position not idle");
        require!(position.debt_usdc.0 == 0, "position has debt");
        require!(
            position.pending_deposit.is_none()
                && position.pending_withdrawal.is_none()
                && position.pending_borrow.is_none(),
            "pending operation"
        );
        require!(position.liquidation_ticket.is_none(), "ticket exists");
        position.owner = new_owner;
        self.positions.insert(position_id, position);
    }

    pub fn retry_derive(&mut self, position_id: u64) {
        let position = self.positions.get(&position_id).expect("position");
        require!(
            position.owner == env::predecessor_account_id(),
            "owner only"
        );
        require!(position.status == PositionStatus::Deriving, "not deriving");
        self.request_derived_key(position_id);
    }

    #[private]
    pub fn on_derived_public_key(&mut self, position_id: u64) {
        let Some(mut position) = self.positions.get(&position_id).cloned() else {
            return;
        };
        match env::promise_result_checked(0, MAX_DERIVED_KEY_LEN) {
            Ok(bytes) => {
                if let Ok(raw) = serde_json::from_slice::<String>(&bytes) {
                    if let Ok(address) = crypto::address_from_derived_pubkey(&raw) {
                        position.deposit_address = Some(crypto::address_hex(&address));
                        position.status = PositionStatus::Idle;
                        self.positions.insert(position_id, position);
                        return;
                    }
                }
                env::log_str("derived_public_key: invalid result");
            }
            Err(_) => env::log_str("derived_public_key: call failed"),
        }
    }

    pub fn register_sepolia_address(&mut self, address: String) {
        let _ = crypto::parse_address_hex(&address).expect("invalid sepolia address");
        self.sepolia_addresses
            .insert(env::predecessor_account_id(), address);
    }

    pub fn record_deposit(
        &mut self,
        position_id: u64,
        header_rlp: Vec<u8>,
        account_proof: Vec<Vec<u8>>,
    ) {
        let mut position = self.positions.get(&position_id).cloned().expect("position");
        require!(position.status == PositionStatus::Idle, "position not idle");
        require!(position.pending_deposit.is_none(), "pending proof");
        require!(position.pending_borrow.is_none(), "pending borrow");
        let deposit_address = position.deposit_address.as_ref().expect("no address");
        let address = crypto::parse_address_hex(deposit_address).expect("bad address");
        let fields = crypto::parse_header_fields(&header_rlp).expect("bad header");
        require!(fields.number > position.last_verified_block, "old proof");
        let account = crypto::verify_account_proof(fields.state_root, address, &account_proof)
            .expect("bad proof")
            .expect("account absent");
        require!(account.is_eoa(), "not a plain EOA");
        require!(account.nonce >= position.last_verified_nonce, "old nonce");
        require!(
            account.balance >= position.collateral_wei.0,
            "balance decreased without a pending withdrawal"
        );
        let header_hash = crypto::keccak256(&header_rlp);
        position.pending_deposit = Some(PendingProof {
            header_hash,
            header_number: fields.number,
            new_balance: U128(account.balance),
            new_nonce: account.nonce,
            kind: ProofKind::Deposit,
            requested_block: env::block_height(),
        });
        self.positions.insert(position_id, position);
        self.request_block_hash(position_id, fields.number);
    }

    pub fn borrow(&mut self, position_id: u64, amount: U128) {
        let mut position = self.positions.get(&position_id).cloned().expect("position");
        require!(
            position.owner == env::predecessor_account_id(),
            "owner only"
        );
        require!(position.status == PositionStatus::Idle, "position not idle");
        require!(!self.config.risk_off, "risk off");
        require!(position.pending_deposit.is_none(), "pending proof");
        require!(position.pending_withdrawal.is_none(), "pending withdrawal");
        require!(position.pending_borrow.is_none(), "pending borrow");
        require!(amount.0 >= self.config.min_borrow_usdc.0, "below minimum");

        self.accrue(&mut position);
        let price = self.fresh_price();
        let collateral_usd =
            math::usd_value(position.collateral_wei.0, price.eth_usd_1e8.0, 18).expect("math");
        let debt_usd =
            math::usd_value(position.debt_usdc.0, price.usdc_usd_1e8.0, 6).expect("math");
        let borrow_usd = math::usd_value(amount.0, price.usdc_usd_1e8.0, 6).expect("math");
        let new_debt_usd = debt_usd.checked_add(borrow_usd).expect("debt overflow");
        let limit =
            math::max_borrow_usd(collateral_usd, self.config.collateral_factor_bps).expect("math");
        require!(new_debt_usd <= limit, "borrow limit exceeded");

        position.pending_borrow = Some(amount);
        let owner = position.owner.clone();
        self.positions.insert(position_id, position);

        let args = serde_json::to_vec(&json!({
            "receiver_id": owner,
            "amount": U128(amount.0),
            "memo": null
        }))
        .expect("json");
        let promise = Promise::new(self.config.usdc.clone()).function_call(
            "ft_transfer".to_string(),
            args,
            NearToken::from_yoctonear(ONE_YOCTO),
            Gas::from_tgas(30),
        );
        let _ = promise.then(self.callback(
            "on_borrow_transfer",
            json!({"position_id": position_id}),
            20,
        ));
    }

    #[private]
    pub fn on_borrow_transfer(&mut self, position_id: u64) {
        let Some(mut position) = self.positions.get(&position_id).cloned() else {
            return;
        };
        let Some(amount) = position.pending_borrow else {
            return;
        };
        if env::promise_result_checked(0, 1_024).is_ok() {
            position.debt_usdc = U128(position.debt_usdc.0.checked_add(amount.0).expect("debt"));
            position.pending_borrow = None;
            self.total_debt_usdc = self
                .total_debt_usdc
                .checked_add(amount.0)
                .expect("total debt");
        } else {
            position.pending_borrow = None;
        }
        self.positions.insert(position_id, position);
    }

    pub fn withdraw(&mut self, position_id: u64, amount: U128) {
        let mut position = self.positions.get(&position_id).cloned().expect("position");
        require!(
            position.owner == env::predecessor_account_id(),
            "owner only"
        );
        require!(position.status == PositionStatus::Idle, "position not idle");
        require!(!self.config.risk_off, "risk off");
        require!(position.pending_deposit.is_none(), "pending proof");
        require!(position.pending_withdrawal.is_none(), "pending withdrawal");
        require!(position.pending_borrow.is_none(), "pending borrow");
        let to = self
            .sepolia_addresses
            .get(&position.owner)
            .cloned()
            .expect("register a sepolia address first");
        let max_gas = self.config.max_gas_wei();
        require!(
            amount.0 >= self.config.min_withdrawal_wei.0,
            "below minimum"
        );
        require!(amount.0 > max_gas, "amount must exceed max gas");

        self.accrue(&mut position);
        let remaining = position
            .collateral_wei
            .0
            .checked_sub(amount.0.checked_add(max_gas).expect("gas"))
            .expect("insufficient collateral");
        let price = self.fresh_price();
        let collateral_usd = math::usd_value(remaining, price.eth_usd_1e8.0, 18).expect("math");
        let debt_usd =
            math::usd_value(position.debt_usdc.0, price.usdc_usd_1e8.0, 6).expect("math");
        let health = math::health_factor_bps(
            collateral_usd,
            debt_usd,
            self.config.liquidation_threshold_bps,
        )
        .expect("math");
        require!(health >= math::BPS, "withdrawal would be unhealthy");
        let limit =
            math::max_borrow_usd(collateral_usd, self.config.collateral_factor_bps).expect("math");
        require!(debt_usd <= limit, "withdrawal would exceed borrow limit");

        let to_addr = crypto::parse_address_hex(&to).expect("bad address");
        let nonce = position.last_verified_nonce;
        let params = crypto::TxParams {
            chain_id: self.config.chain_id,
            nonce,
            max_priority_fee: self.config.max_priority_fee_per_gas.0,
            max_fee: self.config.max_fee_per_gas.0,
            gas_limit: self.config.gas_limit,
            to: to_addr,
            value: amount.0,
            data: &[],
        };
        let signing_hash = crypto::eip1559_signing_hash(&params);
        position.pending_withdrawal = Some(PendingWithdrawal {
            amount_wei: amount,
            max_gas_wei: U128(max_gas),
            nonce,
            to,
            signing_hash,
            raw_tx: None,
            tx_hash: None,
            kind: WithdrawalKind::User,
            requested_block: env::block_height(),
        });
        position.status = PositionStatus::WithdrawalPending;
        self.positions.insert(position_id, position);
        self.request_sign(position_id, signing_hash);
    }

    pub fn confirm_withdrawal(
        &mut self,
        position_id: u64,
        header_rlp: Vec<u8>,
        account_proof: Vec<Vec<u8>>,
    ) {
        let mut position = self.positions.get(&position_id).cloned().expect("position");
        require!(
            position.status == PositionStatus::WithdrawalPending
                || position.status == PositionStatus::LiquidationPending,
            "no pending withdrawal"
        );
        require!(position.pending_deposit.is_none(), "pending proof");
        require!(
            position
                .pending_withdrawal
                .as_ref()
                .map(|withdrawal| withdrawal.raw_tx.is_some())
                .unwrap_or(false),
            "withdrawal not signed"
        );
        let deposit_address = position.deposit_address.as_ref().expect("no address");
        let address = crypto::parse_address_hex(deposit_address).expect("bad address");
        let fields = crypto::parse_header_fields(&header_rlp).expect("bad header");
        require!(fields.number > position.last_verified_block, "old proof");
        let account = crypto::verify_account_proof(fields.state_root, address, &account_proof)
            .expect("bad proof")
            .expect("account absent");
        require!(account.is_eoa(), "not a plain EOA");
        require!(
            account.nonce > position.last_verified_nonce,
            "nonce did not advance"
        );
        let header_hash = crypto::keccak256(&header_rlp);
        position.pending_deposit = Some(PendingProof {
            header_hash,
            header_number: fields.number,
            new_balance: U128(account.balance),
            new_nonce: account.nonce,
            kind: ProofKind::Withdrawal,
            requested_block: env::block_height(),
        });
        self.positions.insert(position_id, position);
        self.request_block_hash(position_id, fields.number);
    }

    #[private]
    pub fn on_sign(&mut self, position_id: u64) {
        let Some(mut position) = self.positions.get(&position_id).cloned() else {
            return;
        };
        let Some(mut pending) = position.pending_withdrawal.clone() else {
            return;
        };
        let Ok(bytes) = env::promise_result_checked(0, MAX_SIGN_RESPONSE_LEN) else {
            self.cancel_withdrawal(&mut position, position_id);
            return;
        };
        let parsed: SignResponse = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                self.cancel_withdrawal(&mut position, position_id);
                return;
            }
        };
        let Some((r, s)) = parsed.r_s() else {
            self.cancel_withdrawal(&mut position, position_id);
            return;
        };
        let Some((signer, normalized_recovery_id, normalized_s)) =
            crypto::recover_signer(&pending.signing_hash, &r, &s, parsed.recovery_id).ok()
        else {
            self.cancel_withdrawal(&mut position, position_id);
            return;
        };
        let deposit_address = position
            .deposit_address
            .as_ref()
            .and_then(|value| crypto::parse_address_hex(value).ok());
        if deposit_address != Some(signer) {
            self.cancel_withdrawal(&mut position, position_id);
            return;
        }
        let to = crypto::parse_address_hex(&pending.to).expect("bad address");
        let params = crypto::TxParams {
            chain_id: self.config.chain_id,
            nonce: pending.nonce,
            max_priority_fee: self.config.max_priority_fee_per_gas.0,
            max_fee: self.config.max_fee_per_gas.0,
            gas_limit: self.config.gas_limit,
            to,
            value: pending.amount_wei.0,
            data: &[],
        };
        let raw_tx = crypto::assemble_signed_tx(&params, normalized_recovery_id, &r, &normalized_s);
        pending.raw_tx = Some(raw_tx.clone());
        pending.tx_hash = Some(crypto::keccak256(&raw_tx));
        position.pending_withdrawal = Some(pending);
        if let Some(ticket) = position.liquidation_ticket.as_mut() {
            ticket.status = TicketStatus::Signed;
        }
        self.positions.insert(position_id, position);
    }

    #[private]
    pub fn on_block_hash(&mut self, position_id: u64) {
        let Some(mut position) = self.positions.get(&position_id).cloned() else {
            return;
        };
        let Some(pending) = position.pending_deposit.clone() else {
            return;
        };
        let returned = env::promise_result_checked(0, MAX_BLOCK_HASH_LEN)
            .ok()
            .and_then(|bytes| decode_optional_hash(&bytes));
        if returned != Some(pending.header_hash) {
            position.pending_deposit = None;
            self.positions.insert(position_id, position);
            env::log_str("block_hash_safe mismatch or failure");
            return;
        }

        match pending.kind {
            ProofKind::Deposit => {
                if pending.new_balance.0 >= position.collateral_wei.0 {
                    let delta = pending.new_balance.0 - position.collateral_wei.0;
                    position.collateral_wei = pending.new_balance;
                    position.last_verified_block = pending.header_number;
                    position.last_verified_nonce = pending.new_nonce;
                    position.pending_deposit = None;
                    self.total_collateral_wei = self
                        .total_collateral_wei
                        .checked_add(delta)
                        .expect("total collateral");
                } else {
                    position.status = PositionStatus::NeedsReview;
                    position.pending_deposit = None;
                }
                self.positions.insert(position_id, position);
            }
            ProofKind::Withdrawal => {
                let Some(withdrawal) = position.pending_withdrawal.clone() else {
                    position.pending_deposit = None;
                    self.positions.insert(position_id, position);
                    return;
                };
                let old_balance = position.collateral_wei.0;
                if pending.new_balance.0 > old_balance {
                    position.status = PositionStatus::NeedsReview;
                    position.pending_deposit = None;
                    self.positions.insert(position_id, position);
                    return;
                }
                let delta = old_balance - pending.new_balance.0;
                let success = delta >= withdrawal.amount_wei.0
                    && delta
                        <= withdrawal
                            .amount_wei
                            .0
                            .saturating_add(withdrawal.max_gas_wei.0);
                let failed = delta < withdrawal.amount_wei.0;

                position.collateral_wei = pending.new_balance;
                position.last_verified_block = pending.header_number;
                position.last_verified_nonce = pending.new_nonce;
                position.pending_deposit = None;
                position.pending_withdrawal = None;
                self.total_collateral_wei = self
                    .total_collateral_wei
                    .checked_sub(delta)
                    .expect("total collateral");

                if success {
                    if withdrawal.kind == WithdrawalKind::Liquidation {
                        if let Some(ticket) = position.liquidation_ticket.as_mut() {
                            ticket.status = TicketStatus::Settled;
                        }
                    }
                    position.status = if position.debt_usdc.0 == 0 && position.collateral_wei.0 == 0
                    {
                        PositionStatus::Closed
                    } else {
                        PositionStatus::Idle
                    };
                } else if failed {
                    if let Some(ticket) = position.liquidation_ticket.as_mut() {
                        ticket.status = TicketStatus::Cancelled;
                    }
                    position.status = PositionStatus::Idle;
                } else {
                    position.status = PositionStatus::NeedsReview;
                }
                self.positions.insert(position_id, position);
            }
        }
    }

    #[payable]
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        if env::predecessor_account_id() != self.config.usdc {
            return PromiseOrValue::Value(amount);
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&msg) else {
            return PromiseOrValue::Value(amount);
        };
        let Some(action) = value.get("action").and_then(|item| item.as_str()) else {
            return PromiseOrValue::Value(amount);
        };
        let Some(position_id) = value.get("position_id").and_then(|item| item.as_u64()) else {
            return PromiseOrValue::Value(amount);
        };
        let Some(mut position) = self.positions.get(&position_id).cloned() else {
            return PromiseOrValue::Value(amount);
        };

        match action {
            "repay" => {
                if sender_id != position.owner {
                    return PromiseOrValue::Value(amount);
                }
                self.accrue(&mut position);
                let accepted = amount.0.min(position.debt_usdc.0);
                if accepted == 0 {
                    return PromiseOrValue::Value(amount);
                }
                position.debt_usdc = U128(position.debt_usdc.0 - accepted);
                self.total_debt_usdc = self
                    .total_debt_usdc
                    .checked_sub(accepted)
                    .expect("total debt");
                self.positions.insert(position_id, position);
                PromiseOrValue::Value(U128(amount.0 - accepted))
            }
            "liquidate" => {
                if position.status != PositionStatus::Idle {
                    return PromiseOrValue::Value(amount);
                }
                if position.pending_deposit.is_some()
                    || position.pending_withdrawal.is_some()
                    || position.pending_borrow.is_some()
                {
                    return PromiseOrValue::Value(amount);
                }
                self.accrue(&mut position);
                let price = self.fresh_price();
                let collateral_usd =
                    math::usd_value(position.collateral_wei.0, price.eth_usd_1e8.0, 18)
                        .expect("math");
                let debt_usd =
                    math::usd_value(position.debt_usdc.0, price.usdc_usd_1e8.0, 6).expect("math");
                let health = math::health_factor_bps(
                    collateral_usd,
                    debt_usd,
                    self.config.liquidation_threshold_bps,
                )
                .expect("math");
                if !math::is_liquidatable(health) {
                    return PromiseOrValue::Value(amount);
                }
                let Some(to) = self.sepolia_addresses.get(&sender_id).cloned() else {
                    return PromiseOrValue::Value(amount);
                };
                let accepted = amount.0.min(position.debt_usdc.0);
                if accepted == 0 {
                    return PromiseOrValue::Value(amount);
                }
                let repay_usd = math::usd_value(accepted, price.usdc_usd_1e8.0, 6).expect("math");
                let seized = math::liquidation_collateral_wei(
                    repay_usd,
                    self.config.liquidation_penalty_bps,
                    price.eth_usd_1e8.0,
                    position.collateral_wei.0,
                )
                .expect("math");
                if seized == 0 {
                    return PromiseOrValue::Value(amount);
                }
                position.debt_usdc = U128(position.debt_usdc.0 - accepted);
                self.total_debt_usdc = self
                    .total_debt_usdc
                    .checked_sub(accepted)
                    .expect("total debt");
                position.liquidation_ticket = Some(LiquidationTicket {
                    liquidator: sender_id.clone(),
                    collateral_wei: U128(seized),
                    repay_usdc: U128(accepted),
                    sepolia_address: to.clone(),
                    status: TicketStatus::PendingSignature,
                });
                self.positions.insert(position_id, position);
                self.request_liquidation_withdrawal(position_id, seized, &to);
                PromiseOrValue::Value(U128(amount.0 - accepted))
            }
            _ => PromiseOrValue::Value(amount),
        }
    }

    pub fn update_price(&mut self, price: Price, signatures: Vec<String>) {
        if let Some(current) = &self.price {
            require!(price.sequence > current.sequence, "old sequence");
        }
        require!(
            price.eth_usd_1e8.0 > 0 && price.usdc_usd_1e8.0 > 0,
            "invalid price"
        );
        let now = env::block_timestamp() / 1_000_000_000;
        require!(price.timestamp <= now.saturating_add(60), "future price");
        require!(
            price
                .timestamp
                .saturating_add(self.config.max_oracle_age_secs)
                >= now,
            "stale price"
        );
        let message = price_message(&price);
        let mut valid = 0u8;
        for (index, key) in self.config.oracle_keys.iter().enumerate() {
            let Some(signature_hex) = signatures.get(index) else {
                continue;
            };
            let Ok(signature) = hex::decode(signature_hex) else {
                continue;
            };
            if signature.len() != 64 {
                continue;
            }
            if key.curve_type() != CurveType::ED25519 {
                continue;
            }
            let key_bytes: [u8; 32] = key.as_bytes()[1..]
                .try_into()
                .expect("ed25519 key length");
            let mut signature_bytes = [0u8; 64];
            signature_bytes.copy_from_slice(&signature);
            if env::ed25519_verify(&signature_bytes, &message, &key_bytes) {
                valid += 1;
            }
        }
        require!(
            valid >= self.config.oracle_threshold,
            "oracle quorum not met"
        );
        self.price = Some(price);
    }

    pub fn set_risk_off(&mut self, risk_off: bool) {
        let caller = env::predecessor_account_id();
        require!(
            caller == self.config.owner || caller == self.config.guardian,
            "not authorized"
        );
        self.config.risk_off = risk_off;
    }

    pub fn set_guardian(&mut self, guardian: AccountId) {
        require!(
            env::predecessor_account_id() == self.config.owner,
            "owner only"
        );
        self.config.guardian = guardian;
    }

    pub fn clear_stuck_proof(&mut self, position_id: u64) {
        let mut position = self.positions.get(&position_id).cloned().expect("position");
        require!(
            position.status == PositionStatus::NeedsReview,
            "not in review"
        );
        require!(
            env::predecessor_account_id() == self.config.owner,
            "owner only"
        );
        require!(
            position
                .pending_deposit
                .as_ref()
                .map(|proof| proof.requested_block + 100_000 < env::block_height())
                .unwrap_or(true),
            "wait for timeout"
        );
        position.pending_deposit = None;
        position.pending_withdrawal = None;
        position.status = PositionStatus::Idle;
        self.positions.insert(position_id, position);
    }

    pub fn get_config(&self) -> Config {
        self.config.clone()
    }

    pub fn get_position(&self, position_id: u64) -> Position {
        self.positions.get(&position_id).expect("position").clone()
    }

    pub fn get_price(&self) -> Option<Price> {
        self.price.clone()
    }

    pub fn get_health(&self, position_id: u64) -> U128 {
        let Some(position) = self.positions.get(&position_id) else {
            return U128(0);
        };
        let Some(price) = self.price.clone() else {
            return U128(0);
        };
        let Some(collateral_usd) =
            math::usd_value(position.collateral_wei.0, price.eth_usd_1e8.0, 18).ok()
        else {
            return U128(0);
        };
        let Some(debt_usd) = math::usd_value(position.debt_usdc.0, price.usdc_usd_1e8.0, 6).ok()
        else {
            return U128(0);
        };
        U128(
            math::health_factor_bps(
                collateral_usd,
                debt_usd,
                self.config.liquidation_threshold_bps,
            )
            .unwrap_or(0),
        )
    }

    fn request_derived_key(&self, position_id: u64) {
        let args = serde_json::to_vec(&json!({
            "path": self.derivation_path(position_id),
            "predecessor": env::current_account_id(),
            "domain_id": 0
        }))
        .expect("json");
        let promise = Promise::new(self.config.mpc.clone()).function_call(
            "derived_public_key".to_string(),
            args,
            NearToken::from_yoctonear(0),
            Gas::from_tgas(10),
        );
        let _ = promise.then(self.callback(
            "on_derived_public_key",
            json!({"position_id": position_id}),
            10,
        ));
    }

    fn request_block_hash(&self, position_id: u64, block_number: u64) {
        let promise = Promise::new(self.config.light_client.clone()).function_call(
            "block_hash_safe".to_string(),
            block_number.to_le_bytes().to_vec(),
            NearToken::from_yoctonear(0),
            Gas::from_tgas(15),
        );
        let _ =
            promise.then(self.callback("on_block_hash", json!({"position_id": position_id}), 40));
    }

    fn request_sign(&self, position_id: u64, signing_hash: [u8; 32]) {
        let args = serde_json::to_vec(&json!({
            "request": {
                "path": self.derivation_path(position_id),
                "payload_v2": { "Ecdsa": hex::encode(signing_hash) },
                "domain_id": 0
            }
        }))
        .expect("json");
        let promise = Promise::new(self.config.mpc.clone()).function_call(
            "sign".to_string(),
            args,
            NearToken::from_yoctonear(ONE_YOCTO),
            Gas::from_tgas(30),
        );
        let _ = promise.then(self.callback("on_sign", json!({"position_id": position_id}), 30));
    }

    fn request_liquidation_withdrawal(&mut self, position_id: u64, amount: u128, to: &str) {
        let mut position = self.positions.get(&position_id).cloned().expect("position");
        let to_address = crypto::parse_address_hex(to).expect("bad address");
        let nonce = position.last_verified_nonce;
        let max_gas = self.config.max_gas_wei();
        let params = crypto::TxParams {
            chain_id: self.config.chain_id,
            nonce,
            max_priority_fee: self.config.max_priority_fee_per_gas.0,
            max_fee: self.config.max_fee_per_gas.0,
            gas_limit: self.config.gas_limit,
            to: to_address,
            value: amount,
            data: &[],
        };
        let signing_hash = crypto::eip1559_signing_hash(&params);
        position.pending_withdrawal = Some(PendingWithdrawal {
            amount_wei: U128(amount),
            max_gas_wei: U128(max_gas),
            nonce,
            to: to.to_string(),
            signing_hash,
            raw_tx: None,
            tx_hash: None,
            kind: WithdrawalKind::Liquidation,
            requested_block: env::block_height(),
        });
        position.status = PositionStatus::LiquidationPending;
        self.positions.insert(position_id, position);
        self.request_sign(position_id, signing_hash);
    }

    fn cancel_withdrawal(&mut self, position: &mut Position, position_id: u64) {
        if let Some(ticket) = position.liquidation_ticket.as_mut() {
            ticket.status = TicketStatus::Cancelled;
        }
        position.pending_withdrawal = None;
        position.status = PositionStatus::Idle;
        self.positions.insert(position_id, position.clone());
    }

    fn callback(&self, method: &str, args: serde_json::Value, tgas: u64) -> Promise {
        Promise::new(env::current_account_id()).function_call(
            method.to_string(),
            serde_json::to_vec(&args).expect("json"),
            NearToken::from_yoctonear(0),
            Gas::from_tgas(tgas),
        )
    }

    fn derivation_path(&self, position_id: u64) -> String {
        format!(
            "native-credit:v1:{}:position:{}",
            self.config.chain_id, position_id
        )
    }

    fn fresh_price(&self) -> Price {
        let price = self.price.clone().expect("price not set");
        let now = env::block_timestamp() / 1_000_000_000;
        require!(
            price
                .timestamp
                .saturating_add(self.config.max_oracle_age_secs)
                >= now,
            "stale price"
        );
        price
    }

    fn accrue(&mut self, position: &mut Position) {
        let now = env::block_timestamp() / 1_000_000_000;
        let elapsed = now.saturating_sub(position.last_accrual_secs);
        let new_debt =
            math::accrue_interest(position.debt_usdc.0, self.config.interest_rate_bps, elapsed)
                .expect("interest overflow");
        self.total_debt_usdc = self
            .total_debt_usdc
            .checked_sub(position.debt_usdc.0)
            .expect("total debt")
            .checked_add(new_debt)
            .expect("total debt");
        position.debt_usdc = U128(new_debt);
        position.last_accrual_secs = now;
    }
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct SignResponse {
    pub big_r: AffinePoint,
    pub s: Scalar,
    pub recovery_id: u8,
}

impl SignResponse {
    fn r_s(&self) -> Option<([u8; 32], [u8; 32])> {
        let point = hex::decode(
            self.big_r
                .affine_point
                .strip_prefix("0x")
                .unwrap_or(&self.big_r.affine_point),
        )
        .ok()?;
        if point.len() != 33 {
            return None;
        }
        let scalar =
            hex::decode(self.s.scalar.strip_prefix("0x").unwrap_or(&self.s.scalar)).ok()?;
        if scalar.len() != 32 {
            return None;
        }
        let mut r = [0u8; 32];
        r.copy_from_slice(&point[1..]);
        let mut s = [0u8; 32];
        s.copy_from_slice(&scalar);
        Some((r, s))
    }
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct AffinePoint {
    pub affine_point: String,
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct Scalar {
    pub scalar: String,
}

fn decode_optional_hash(bytes: &[u8]) -> Option<[u8; 32]> {
    if bytes.len() != 33 || bytes[0] == 0 {
        return None;
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes[1..]);
    Some(hash)
}

fn price_message(price: &Price) -> Vec<u8> {
    format!(
        "native-credit:v1:price:{}:{}:{}:{}",
        price.eth_usd_1e8.0, price.usdc_usd_1e8.0, price.timestamp, price.sequence
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{get_logs, VMContextBuilder};
    use near_sdk::{testing_env, PromiseResult};
    use serde_json::json;

    fn config(owner: AccountId) -> Config {
        Config {
            owner,
            guardian: "guardian.testnet".parse().unwrap(),
            usdc: "usdc.testnet".parse().unwrap(),
            light_client: "client-eth2.sepolia.testnet".parse().unwrap(),
            mpc: "v1.signer-prod.testnet".parse().unwrap(),
            oracle_keys: vec![PublicKey::from_parts(CurveType::ED25519, vec![7u8; 32]).unwrap()],
            oracle_threshold: 1,
            collateral_factor_bps: 7_500,
            liquidation_threshold_bps: 8_000,
            liquidation_penalty_bps: 800,
            interest_rate_bps: 500,
            max_oracle_age_secs: 300,
            min_withdrawal_wei: U128(10u128.pow(16)),
            min_borrow_usdc: U128(1_000_000),
            risk_off: false,
            chain_id: 11155111,
            gas_limit: 21_000,
            max_fee_per_gas: U128(2_000_000_000),
            max_priority_fee_per_gas: U128(1_000_000_000),
        }
    }

    fn context(predecessor: &str, deposit: u128) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor.parse().unwrap())
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .block_timestamp(1_700_000_000_000_000_000);
        builder
    }

    #[test]
    fn init_accepts_deployed_config() {
        let json = r#"{
          "owner": "native-credit.testnet",
          "guardian": "native-credit.testnet",
          "usdc": "3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af",
          "light_client": "client-eth2.sepolia.testnet",
          "mpc": "v1.signer-prod.testnet",
          "oracle_keys": [
            "ed25519:Gj9qzpQPtCeuVdRpw9FwwuGosypMsEdxo3D1J3ZBV2NA",
            "ed25519:A8rmbsShJT3AyHTBuZxPMHv7hiDLWzS7QP9xRcX8G4Sa",
            "ed25519:3qgLy6HNdoWCbKAp4AwevhvJA3WEXQLCyQy9hxvgAb6h"
          ],
          "oracle_threshold": 2,
          "collateral_factor_bps": 7500,
          "liquidation_threshold_bps": 8000,
          "liquidation_penalty_bps": 800,
          "interest_rate_bps": 500,
          "max_oracle_age_secs": 300,
          "min_withdrawal_wei": "10000000000000000",
          "min_borrow_usdc": "1000000",
          "risk_off": false,
          "chain_id": 11155111,
          "gas_limit": 21000,
          "max_fee_per_gas": "2000000000",
          "max_priority_fee_per_gas": "1000000000"
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        let ctx = context("native-credit.testnet", 0);
        testing_env!(ctx.build());
        let contract = Contract::new(config);
        assert_eq!(contract.get_config().oracle_threshold, 2);
        assert_eq!(contract.get_config().oracle_keys.len(), 3);
    }

    #[test]
    fn owner_can_transfer_an_idle_position() {
        let owner: AccountId = "alice.testnet".parse().unwrap();
        testing_env!(context("alice.testnet", 0).build());
        let mut contract = Contract::new(config(owner.clone()));
        contract.positions.insert(
            1,
            Position {
                owner,
                status: PositionStatus::Idle,
                deposit_address: Some(
                    "0x0000000000000000000000000000000000000002".to_string(),
                ),
                collateral_wei: U128(10u128.pow(18)),
                last_verified_block: 100,
                last_verified_nonce: 0,
                debt_usdc: U128(0),
                last_accrual_secs: 1_700_000_000,
                pending_deposit: None,
                pending_withdrawal: None,
                pending_borrow: None,
                liquidation_ticket: None,
            },
        );
        let new_owner: AccountId = "bob.testnet".parse().unwrap();
        contract.transfer_position(1, new_owner.clone());
        assert_eq!(contract.get_position(1).owner, new_owner);
    }

    #[test]
    fn update_price_accepts_ed25519_quorum() {
        let mut cfg = config("owner.testnet".parse().unwrap());
        cfg.oracle_keys = vec![
            "ed25519:Gj9qzpQPtCeuVdRpw9FwwuGosypMsEdxo3D1J3ZBV2NA"
                .parse()
                .unwrap(),
            "ed25519:A8rmbsShJT3AyHTBuZxPMHv7hiDLWzS7QP9xRcX8G4Sa"
                .parse()
                .unwrap(),
        ];
        cfg.oracle_threshold = 2;
        let mut contract = Contract::new(cfg);
        testing_env!(context("owner.testnet", 0).build());
        let price = Price {
            eth_usd_1e8: U128(300_000_000_000),
            usdc_usd_1e8: U128(100_000_000),
            timestamp: 1_700_000_000,
            sequence: 1,
        };
        let signatures = vec![
            "8b390b5789edf8f28becd8ae84e5877fec2e882723bdc9203f442c69290774ebbb91064056f2bebc82de3e6acb1559ad2a77b8926d865ef1a8cf38b8af711704".to_string(),
            "d2e55dc99682672f2f14c770547c16e478996abca803a3397f9e46f1e9a54eb8a070e0c3e420073200945bcb26a3d7771999ddb89c1ad10d38caf846f163b80c".to_string(),
            String::new(),
        ];
        contract.update_price(price, signatures);
        let stored = contract.get_price().expect("stored price");
        assert_eq!(stored.eth_usd_1e8.0, 300_000_000_000);
        assert_eq!(stored.sequence, 1);
    }

    #[test]
    fn creates_position_with_storage_deposit() {
        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", STORAGE_DEPOSIT_YOCTO);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner.clone()));
        let id = contract.create_position();
        assert_eq!(id, 1);
        let position = contract.get_position(id);
        assert_eq!(position.owner, owner);
        assert_eq!(position.status, PositionStatus::Deriving);
        assert_eq!(contract.next_position_id, 2);
        assert_eq!(get_logs().len(), 0);
    }

    #[test]
    fn rejects_missing_storage_deposit() {
        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", 1);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contract.create_position();
        }));
        assert!(result.is_err());
    }

    #[test]
    fn risk_off_is_authorized_and_does_not_pause_repay_path() {
        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", 0);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner.clone()));
        contract.set_risk_off(true);
        assert!(contract.config.risk_off);

        let ctx = context("mallory.testnet", 0);
        testing_env!(ctx.build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contract.set_risk_off(false);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn registered_sepolia_address_is_stored() {
        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", 0);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner));
        contract.register_sepolia_address("0x0000000000000000000000000000000000000001".to_string());
        assert_eq!(
            contract
                .sepolia_addresses
                .get(&"alice.testnet".parse::<AccountId>().unwrap())
                .cloned(),
            Some("0x0000000000000000000000000000000000000001".to_string())
        );
    }

    #[test]
    fn wrong_token_is_refunded() {
        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", 0);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner));
        let result = contract.ft_on_transfer(
            "alice.testnet".parse().unwrap(),
            U128(100),
            "{}".to_string(),
        );
        match result {
            PromiseOrValue::Value(value) => assert_eq!(value.0, 100),
            _ => panic!("expected refund"),
        }
    }

    #[test]
    fn persisted_position_can_record_a_real_sepolia_proof() {
        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", 0);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner.clone()));

        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/sepolia_12058306.json")).unwrap();
        let address = "0x0000000000000000000000000000000000000001";
        let account = &vectors["accounts"][address];
        let proof: Vec<Vec<u8>> = account["accountProof"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let raw = value.as_str().unwrap().strip_prefix("0x").unwrap();
                hex::decode(raw).unwrap()
            })
            .collect();
        let header = {
            let raw = vectors["headerRlp"]
                .as_str()
                .unwrap()
                .strip_prefix("0x")
                .unwrap();
            hex::decode(raw).unwrap()
        };

        let position = Position {
            owner,
            status: PositionStatus::Idle,
            deposit_address: Some(address.to_string()),
            collateral_wei: U128(0),
            last_verified_block: 0,
            last_verified_nonce: 0,
            debt_usdc: U128(0),
            last_accrual_secs: 0,
            pending_deposit: None,
            pending_withdrawal: None,
            pending_borrow: None,
            liquidation_ticket: None,
        };
        contract.positions.insert(1, position);
        contract.record_deposit(1, header, proof);
        let pending = contract
            .get_position(1)
            .pending_deposit
            .expect("pending proof");
        assert_eq!(pending.new_balance.0, 0x418c84296add147a);
        assert_eq!(pending.new_nonce, 0);
    }

    #[test]
    fn liquidation_math_seizes_penalty() {
        let seized =
            math::liquidation_collateral_wei(100_000_000, 800, 300_000_000_000, 10u128.pow(18))
                .unwrap();
        assert_eq!(seized, 360_000_000_000_000);
    }
    #[test]
    fn deposit_credits_after_light_client_confirms_header_hash() {
        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", 0);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner.clone()));

        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/sepolia_12058306.json")).unwrap();
        let address = "0x0000000000000000000000000000000000000001";
        let account = &vectors["accounts"][address];
        let proof: Vec<Vec<u8>> = account["accountProof"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let raw = value.as_str().unwrap().strip_prefix("0x").unwrap();
                hex::decode(raw).unwrap()
            })
            .collect();
        let header = {
            let raw = vectors["headerRlp"]
                .as_str()
                .unwrap()
                .strip_prefix("0x")
                .unwrap();
            hex::decode(raw).unwrap()
        };

        let position = Position {
            owner,
            status: PositionStatus::Idle,
            deposit_address: Some(address.to_string()),
            collateral_wei: U128(0),
            last_verified_block: 0,
            last_verified_nonce: 0,
            debt_usdc: U128(0),
            last_accrual_secs: 0,
            pending_deposit: None,
            pending_withdrawal: None,
            pending_borrow: None,
            liquidation_ticket: None,
        };
        contract.positions.insert(1, position);
        contract.record_deposit(1, header, proof);
        let pending = contract
            .get_position(1)
            .pending_deposit
            .expect("pending proof");
        let mut result = Vec::with_capacity(33);
        result.push(1);
        result.extend_from_slice(&pending.header_hash);

        testing_env!(
            ctx.build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            vec![PromiseResult::Successful(result)],
        );
        contract.on_block_hash(1);

        let position = contract.get_position(1);
        assert_eq!(position.collateral_wei.0, 0x418c84296add147a);
        assert_eq!(position.last_verified_block, 12058306);
        assert_eq!(position.last_verified_nonce, 0);
        assert!(position.pending_deposit.is_none());
        assert_eq!(contract.total_collateral_wei, 0x418c84296add147a);
    }

    #[test]
    fn deposit_is_not_credited_when_light_client_hash_differs() {
        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", 0);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner.clone()));

        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/sepolia_12058306.json")).unwrap();
        let address = "0x0000000000000000000000000000000000000001";
        let proof: Vec<Vec<u8>> = vectors["accounts"][address]["accountProof"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let raw = value.as_str().unwrap().strip_prefix("0x").unwrap();
                hex::decode(raw).unwrap()
            })
            .collect();
        let header = {
            let raw = vectors["headerRlp"]
                .as_str()
                .unwrap()
                .strip_prefix("0x")
                .unwrap();
            hex::decode(raw).unwrap()
        };

        contract.positions.insert(
            1,
            Position {
                owner,
                status: PositionStatus::Idle,
                deposit_address: Some(address.to_string()),
                collateral_wei: U128(0),
                last_verified_block: 0,
                last_verified_nonce: 0,
                debt_usdc: U128(0),
                last_accrual_secs: 0,
                pending_deposit: None,
                pending_withdrawal: None,
                pending_borrow: None,
                liquidation_ticket: None,
            },
        );
        contract.record_deposit(1, header, proof);

        let mut result = Vec::with_capacity(33);
        result.push(1);
        result.extend_from_slice(&[0u8; 32]);
        testing_env!(
            ctx.build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            vec![PromiseResult::Successful(result)],
        );
        contract.on_block_hash(1);

        let position = contract.get_position(1);
        assert_eq!(position.collateral_wei.0, 0);
        assert!(position.pending_deposit.is_none());
        assert_eq!(contract.total_collateral_wei, 0);
    }

    #[test]
    fn withdrawal_signature_builds_raw_transaction() {
        use k256::ecdsa::SigningKey;

        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", 0);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner.clone()));
        let now = env::block_timestamp() / 1_000_000_000;

        let signing_key = SigningKey::from_slice(&[9u8; 32]).unwrap();
        let point = signing_key.verifying_key().to_encoded_point(false);
        let hash = crypto::keccak256(&point.as_bytes()[1..]);
        let mut signer_address = [0u8; 20];
        signer_address.copy_from_slice(&hash[12..]);
        let signer_address_hex = crypto::address_hex(&signer_address);
        let to = "0x0000000000000000000000000000000000000002".to_string();
        contract.sepolia_addresses.insert(owner.clone(), to.clone());
        contract.price = Some(Price {
            eth_usd_1e8: U128(300_000_000_000),
            usdc_usd_1e8: U128(100_000_000),
            timestamp: now,
            sequence: 1,
        });
        contract.positions.insert(
            1,
            Position {
                owner: owner.clone(),
                status: PositionStatus::Idle,
                deposit_address: Some(signer_address_hex),
                collateral_wei: U128(10u128.pow(18)),
                last_verified_block: 100,
                last_verified_nonce: 0,
                debt_usdc: U128(0),
                last_accrual_secs: now,
                pending_deposit: None,
                pending_withdrawal: None,
                pending_borrow: None,
                liquidation_ticket: None,
            },
        );
        contract.total_collateral_wei = 10u128.pow(18);

        contract.withdraw(1, U128(10u128.pow(16)));
        let pending = contract
            .get_position(1)
            .pending_withdrawal
            .expect("pending withdrawal");
        let (signature, recovery_id) = signing_key
            .sign_prehash_recoverable(&pending.signing_hash)
            .unwrap();
        let bytes = signature.to_bytes();
        let r = hex::encode(&bytes[..32]);
        let s = hex::encode(&bytes[32..]);
        let recovery = recovery_id.to_byte();
        let prefix = if recovery % 2 == 0 { 2 } else { 3 };
        let response = json!({
            "big_r": { "affine_point": format!("0x{:02x}{}", prefix, r) },
            "s": { "scalar": format!("0x{}", s) },
            "recovery_id": recovery,
        });
        let result = serde_json::to_vec(&response).unwrap();

        testing_env!(
            ctx.build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            vec![PromiseResult::Successful(result)],
        );
        contract.on_sign(1);

        let position = contract.get_position(1);
        assert_eq!(position.status, PositionStatus::WithdrawalPending);
        let pending = position.pending_withdrawal.expect("signed withdrawal");
        assert!(pending.raw_tx.is_some());
        assert!(pending.tx_hash.is_some());
        assert_eq!(pending.raw_tx.unwrap()[0], 2);
    }

    #[test]
    fn withdrawal_signature_from_other_key_cancels() {
        use k256::ecdsa::SigningKey;

        let owner: AccountId = "alice.testnet".parse().unwrap();
        let ctx = context("alice.testnet", 0);
        testing_env!(ctx.build());
        let mut contract = Contract::new(config(owner.clone()));
        let now = env::block_timestamp() / 1_000_000_000;

        let expected_key = SigningKey::from_slice(&[9u8; 32]).unwrap();
        let point = expected_key.verifying_key().to_encoded_point(false);
        let hash = crypto::keccak256(&point.as_bytes()[1..]);
        let mut expected_address = [0u8; 20];
        expected_address.copy_from_slice(&hash[12..]);
        let to = "0x0000000000000000000000000000000000000002".to_string();
        contract.sepolia_addresses.insert(owner.clone(), to);
        contract.price = Some(Price {
            eth_usd_1e8: U128(300_000_000_000),
            usdc_usd_1e8: U128(100_000_000),
            timestamp: now,
            sequence: 1,
        });
        contract.positions.insert(
            1,
            Position {
                owner: owner.clone(),
                status: PositionStatus::Idle,
                deposit_address: Some(crypto::address_hex(&expected_address)),
                collateral_wei: U128(10u128.pow(18)),
                last_verified_block: 100,
                last_verified_nonce: 0,
                debt_usdc: U128(0),
                last_accrual_secs: now,
                pending_deposit: None,
                pending_withdrawal: None,
                pending_borrow: None,
                liquidation_ticket: None,
            },
        );
        contract.total_collateral_wei = 10u128.pow(18);
        contract.withdraw(1, U128(10u128.pow(16)));
        let pending = contract
            .get_position(1)
            .pending_withdrawal
            .expect("pending withdrawal");

        let wrong_key = SigningKey::from_slice(&[10u8; 32]).unwrap();
        let (signature, recovery_id) = wrong_key
            .sign_prehash_recoverable(&pending.signing_hash)
            .unwrap();
        let bytes = signature.to_bytes();
        let recovery = recovery_id.to_byte();
        let prefix = if recovery % 2 == 0 { 2 } else { 3 };
        let response = json!({
            "big_r": { "affine_point": format!("0x{:02x}{}", prefix, hex::encode(&bytes[..32])) },
            "s": { "scalar": format!("0x{}", hex::encode(&bytes[32..])) },
            "recovery_id": recovery,
        });
        let result = serde_json::to_vec(&response).unwrap();

        testing_env!(
            ctx.build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            vec![PromiseResult::Successful(result)],
        );
        contract.on_sign(1);

        let position = contract.get_position(1);
        assert_eq!(position.status, PositionStatus::Idle);
        assert!(position.pending_withdrawal.is_none());
    }
}
