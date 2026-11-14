use ed25519_dalek::{Signer as _, SigningKey as EdSigningKey};
use k256::ecdsa::SigningKey;
use native_credit::crypto::{address_hex, keccak256, EMPTY_CODE_HASH, EMPTY_TRIE_ROOT};
use native_credit::{Config, Contract, PositionStatus, Price};
use near_sdk::json_types::U128;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::{testing_env, AccountId, CurveType, NearToken, PromiseResult, PublicKey};
use rlp::RlpStream;
use serde_json::json;

const OWNER: &str = "alice.testnet";
const LIQUIDATOR: &str = "liquidator.testnet";
const USDC: &str = "usdc.testnet";
const CONTRACT: &str = "contract.testnet";
const NOW_SECS: u64 = 1_700_000_000;
const STORAGE_DEPOSIT: u128 = 20_000_000_000_000_000_000_000;
const COLLATERAL: u128 = 50_000_000_000_000_000;
const GAS_COST: u128 = 100_000_000_000_000;

fn account(account_id: &str) -> AccountId {
    account_id.parse().unwrap()
}

fn context(predecessor: &str, deposit: u128, height: u64, timestamp: u64) -> VMContextBuilder {
    let mut builder = VMContextBuilder::new();
    builder
        .current_account_id(account(CONTRACT))
        .predecessor_account_id(account(predecessor))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .block_height(height)
        .block_timestamp(timestamp * 1_000_000_000);
    builder
}

fn set_env(ctx: VMContextBuilder, results: Vec<PromiseResult>) {
    testing_env!(
        ctx.build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        results
    );
}

fn append_u128(stream: &mut RlpStream, value: u128) {
    let bytes = value.to_be_bytes();
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    stream.append(&bytes[first..].to_vec());
}

fn account_rlp(nonce: u64, balance: u128) -> Vec<u8> {
    let mut stream = RlpStream::new_list(4);
    stream.append(&nonce);
    append_u128(&mut stream, balance);
    stream.append(&EMPTY_TRIE_ROOT.to_vec());
    stream.append(&EMPTY_CODE_HASH.to_vec());
    stream.out().to_vec()
}

fn header_rlp(number: u64, state_root: &[u8; 32]) -> Vec<u8> {
    let mut stream = RlpStream::new_list(9);
    stream.append(&[0u8; 32].to_vec());
    stream.append(&[0u8; 32].to_vec());
    stream.append(&[0u8; 20].to_vec());
    stream.append(&state_root.to_vec());
    stream.append(&[0u8; 32].to_vec());
    stream.append(&[0u8; 32].to_vec());
    stream.append(&[0u8; 256].to_vec());
    stream.append(&[0u8; 1].to_vec());
    stream.append(&number.to_be_bytes().to_vec());
    stream.out().to_vec()
}

fn synthetic_proof(
    address: [u8; 20],
    nonce: u64,
    balance: u128,
    number: u64,
) -> (Vec<u8>, Vec<Vec<u8>>, [u8; 32]) {
    let account = account_rlp(nonce, balance);
    let key = keccak256(&address);
    let mut path = Vec::with_capacity(33);
    path.push(0x20);
    path.extend_from_slice(&key);
    let mut leaf = RlpStream::new_list(2);
    leaf.append(&path);
    leaf.append(&account);
    let leaf = leaf.out().to_vec();
    let state_root = keccak256(&leaf);
    let header = header_rlp(number, &state_root);
    let header_hash = keccak256(&header);
    (header, vec![leaf], header_hash)
}

fn hash_result(hash: &[u8; 32]) -> Vec<u8> {
    let mut result = Vec::with_capacity(33);
    result.push(1);
    result.extend_from_slice(hash);
    result
}

fn secp_address(key: &SigningKey) -> [u8; 20] {
    let point = key.verifying_key().to_encoded_point(false);
    let hash = keccak256(&point.as_bytes()[1..]);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    address
}

fn derived_pubkey(key: &SigningKey) -> String {
    let point = key.verifying_key().to_encoded_point(false);
    format!(
        "secp256k1:{}",
        bs58::encode(&point.as_bytes()[1..]).into_string()
    )
}

fn config(oracle_key: PublicKey) -> Config {
    Config {
        owner: account(OWNER),
        guardian: account("guardian.testnet"),
        usdc: account(USDC),
        light_client: account("light-client.testnet"),
        mpc: account("mpc.testnet"),
        oracle_keys: vec![oracle_key],
        oracle_threshold: 1,
        collateral_factor_bps: 7_500,
        liquidation_threshold_bps: 8_000,
        liquidation_penalty_bps: 800,
        interest_rate_bps: 500,
        max_oracle_age_secs: 300,
        min_withdrawal_wei: U128(10_000_000_000_000_000),
        min_borrow_usdc: U128(1_000_000),
        risk_off: false,
        chain_id: 11_155_111,
        gas_limit: 250_000,
        max_fee_per_gas: U128(2_000_000_000),
        max_priority_fee_per_gas: U128(1_000_000_000),
    }
}

fn publish_price(contract: &mut Contract, oracle: &EdSigningKey, eth_usd_1e8: u128, sequence: u64) {
    let price = Price {
        eth_usd_1e8: U128(eth_usd_1e8),
        usdc_usd_1e8: U128(100_000_000),
        timestamp: NOW_SECS,
        sequence,
    };
    let message = format!(
        "native-credit:v1:price:{}:{}:{}:{}",
        price.eth_usd_1e8.0, price.usdc_usd_1e8.0, price.timestamp, price.sequence
    );
    let signature = oracle.sign(message.as_bytes());
    contract.update_price(price, vec![hex::encode(signature.to_bytes())]);
}

fn bootstrap(
    eth_usd_1e8: u128,
    borrow_amount: u128,
) -> (Contract, SigningKey, [u8; 20], EdSigningKey) {
    let owner_key = SigningKey::from_slice(&[9u8; 32]).unwrap();
    let oracle = EdSigningKey::from_bytes(&[7u8; 32]);
    let oracle_key = PublicKey::from_parts(
        CurveType::ED25519,
        oracle.verifying_key().to_bytes().to_vec(),
    )
    .unwrap();

    set_env(context(OWNER, 0, 1, NOW_SECS), vec![]);
    let mut contract = Contract::new(config(oracle_key));
    publish_price(&mut contract, &oracle, eth_usd_1e8, 1);

    set_env(context(OWNER, STORAGE_DEPOSIT, 2, NOW_SECS), vec![]);
    assert_eq!(contract.create_position(), 1);

    let derived = serde_json::to_vec(&derived_pubkey(&owner_key)).unwrap();
    set_env(
        context(CONTRACT, 0, 3, NOW_SECS),
        vec![PromiseResult::Successful(derived)],
    );
    contract.on_derived_public_key(1);
    let address = secp_address(&owner_key);
    assert_eq!(
        contract.get_position(1).deposit_address.as_deref(),
        Some(address_hex(&address).as_str())
    );

    let (header, proof, header_hash) = synthetic_proof(address, 0, COLLATERAL, 1_000);
    set_env(context(OWNER, 0, 4, NOW_SECS), vec![]);
    contract.record_deposit(1, header, proof);
    set_env(
        context(CONTRACT, 0, 5, NOW_SECS),
        vec![PromiseResult::Successful(hash_result(&header_hash))],
    );
    contract.on_block_hash(1);
    assert_eq!(contract.get_position(1).collateral_wei.0, COLLATERAL);

    set_env(context(OWNER, 0, 6, NOW_SECS), vec![]);
    contract.borrow(1, U128(borrow_amount));
    set_env(
        context(CONTRACT, 0, 7, NOW_SECS),
        vec![PromiseResult::Successful(Vec::new())],
    );
    contract.on_borrow_transfer(1);
    assert_eq!(contract.get_position(1).debt_usdc.0, borrow_amount);

    (contract, owner_key, address, oracle)
}

fn sign_withdrawal(contract: &mut Contract, owner_key: &SigningKey) {
    let pending = contract
        .get_position(1)
        .pending_withdrawal
        .expect("pending withdrawal");
    let (signature, recovery_id) = owner_key
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
    set_env(
        context(CONTRACT, 0, 8, NOW_SECS),
        vec![PromiseResult::Successful(
            serde_json::to_vec(&response).unwrap(),
        )],
    );
    contract.on_sign(1);
    let pending = contract
        .get_position(1)
        .pending_withdrawal
        .expect("signed withdrawal");
    assert!(pending.raw_tx.is_some());
    assert!(pending.tx_hash.is_some());
}

#[test]
fn local_e2e_deposit_borrow_repay_withdraw_confirm() {
    let (mut contract, owner_key, address, _oracle) = bootstrap(300_000_000_000, 10_000_000);
    assert_eq!(contract.get_positions_by_owner(account(OWNER)).len(), 1);
    assert!(contract
        .get_positions_by_owner(account("bob.testnet"))
        .is_empty());

    set_env(context(USDC, 0, 9, NOW_SECS), vec![]);
    let refund = contract.ft_on_transfer(
        account(OWNER),
        U128(10_000_000),
        json!({"action": "repay", "position_id": 1}).to_string(),
    );
    assert!(matches!(refund, near_sdk::PromiseOrValue::Value(U128(0))));
    assert_eq!(contract.get_position(1).debt_usdc.0, 0);

    set_env(context(OWNER, 0, 10, NOW_SECS), vec![]);
    contract.register_sepolia_address("0x0000000000000000000000000000000000000002".to_string());
    contract.withdraw(1, U128(10_000_000_000_000_000));
    sign_withdrawal(&mut contract, &owner_key);

    let balance_after = COLLATERAL - 10_000_000_000_000_000 - GAS_COST;
    let (header, proof, header_hash) = synthetic_proof(address, 1, balance_after, 1_001);
    set_env(context(OWNER, 0, 11, NOW_SECS), vec![]);
    contract.confirm_withdrawal(1, header, proof);
    set_env(
        context(CONTRACT, 0, 12, NOW_SECS),
        vec![PromiseResult::Successful(hash_result(&header_hash))],
    );
    contract.on_block_hash(1);

    let position = contract.get_position(1);
    assert_eq!(position.status, PositionStatus::Idle);
    assert_eq!(position.collateral_wei.0, balance_after);
    assert_eq!(position.last_verified_nonce, 1);
    assert!(position.pending_withdrawal.is_none());
    assert!(position.pending_deposit.is_none());
}

#[test]
fn local_e2e_liquidation_repays_debt_and_settles_ticket() {
    let (mut contract, owner_key, address, oracle) = bootstrap(300_000_000_000, 4_400_000);

    set_env(context(OWNER, 0, 8, NOW_SECS), vec![]);
    publish_price(&mut contract, &oracle, 10_000_000_000, 2);

    set_env(context(LIQUIDATOR, 0, 9, NOW_SECS), vec![]);
    contract.register_sepolia_address("0x0000000000000000000000000000000000000003".to_string());

    set_env(context(USDC, 0, 10, NOW_SECS), vec![]);
    let refund = contract.ft_on_transfer(
        account(LIQUIDATOR),
        U128(4_400_000),
        json!({"action": "liquidate", "position_id": 1}).to_string(),
    );
    assert!(matches!(refund, near_sdk::PromiseOrValue::Value(U128(0))));
    assert_eq!(contract.get_position(1).debt_usdc.0, 0);
    let ticket = contract
        .get_position(1)
        .liquidation_ticket
        .expect("liquidation ticket");
    assert_eq!(ticket.status, native_credit::TicketStatus::PendingSignature);

    sign_withdrawal(&mut contract, &owner_key);
    assert_eq!(
        contract
            .get_position(1)
            .liquidation_ticket
            .expect("ticket")
            .status,
        native_credit::TicketStatus::Signed
    );

    let seized = contract
        .get_position(1)
        .liquidation_ticket
        .expect("ticket")
        .collateral_wei
        .0;
    let balance_after = COLLATERAL - seized - GAS_COST;
    let (header, proof, header_hash) = synthetic_proof(address, 1, balance_after, 1_001);
    set_env(context(OWNER, 0, 12, NOW_SECS), vec![]);
    contract.confirm_withdrawal(1, header, proof);
    set_env(
        context(CONTRACT, 0, 13, NOW_SECS),
        vec![PromiseResult::Successful(hash_result(&header_hash))],
    );
    contract.on_block_hash(1);

    let position = contract.get_position(1);
    assert_eq!(position.status, PositionStatus::Idle);
    assert_eq!(position.debt_usdc.0, 0);
    assert_eq!(position.collateral_wei.0, balance_after);
    assert_eq!(
        position.liquidation_ticket.expect("ticket").status,
        native_credit::TicketStatus::Settled
    );
}
