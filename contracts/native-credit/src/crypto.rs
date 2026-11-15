use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use rlp::{Rlp, RlpStream};
use tiny_keccak::{Hasher, Keccak};

pub const EMPTY_TRIE_ROOT: [u8; 32] = [
    0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6, 0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
    0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0, 0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21,
];

pub const EMPTY_CODE_HASH: [u8; 32] = [
    0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03, 0xc0,
    0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85, 0xa4, 0x70,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    InvalidRlp,
    InvalidHeader,
    InvalidProof,
    InvalidAccount,
    BalanceOverflow,
    InvalidPublicKey,
    InvalidSignature,
}

pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    let mut out = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut out);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderFields {
    pub number: u64,
    pub state_root: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountState {
    pub nonce: u64,
    pub balance: u128,
    pub storage_root: [u8; 32],
    pub code_hash: [u8; 32],
}

impl AccountState {
    pub fn is_eoa(&self) -> bool {
        self.code_hash == EMPTY_CODE_HASH && self.storage_root == EMPTY_TRIE_ROOT
    }
}

pub fn parse_header_fields(header_rlp: &[u8]) -> Result<HeaderFields, CryptoError> {
    let rlp = Rlp::new(header_rlp);
    if !rlp.is_list() {
        return Err(CryptoError::InvalidHeader);
    }
    let item_count = rlp.item_count().map_err(|_| CryptoError::InvalidRlp)?;
    if item_count < 9 {
        return Err(CryptoError::InvalidHeader);
    }

    let state_root_bytes = rlp
        .at(3)
        .map_err(|_| CryptoError::InvalidHeader)?
        .data()
        .map_err(|_| CryptoError::InvalidHeader)?;
    if state_root_bytes.len() != 32 {
        return Err(CryptoError::InvalidHeader);
    }
    let mut state_root = [0u8; 32];
    state_root.copy_from_slice(state_root_bytes);

    let number_bytes = rlp
        .at(8)
        .map_err(|_| CryptoError::InvalidHeader)?
        .data()
        .map_err(|_| CryptoError::InvalidHeader)?;
    if number_bytes.len() > 8 {
        return Err(CryptoError::InvalidHeader);
    }
    let mut number_buf = [0u8; 8];
    number_buf[8 - number_bytes.len()..].copy_from_slice(number_bytes);
    let number = u64::from_be_bytes(number_buf);

    Ok(HeaderFields { number, state_root })
}

pub fn verify_account_proof(
    state_root: [u8; 32],
    address: [u8; 20],
    proof: &[Vec<u8>],
) -> Result<Option<AccountState>, CryptoError> {
    if proof.is_empty() {
        return Err(CryptoError::InvalidProof);
    }
    let key = keccak256(&address);
    let value = verify_trie_proof(state_root, &key, proof)?;
    if value.is_empty() {
        return Ok(None);
    }
    decode_account(&value).map(Some)
}

fn decode_account(account_rlp: &[u8]) -> Result<AccountState, CryptoError> {
    let rlp = Rlp::new(account_rlp);
    if !rlp.is_list() || rlp.item_count().map_err(|_| CryptoError::InvalidAccount)? != 4 {
        return Err(CryptoError::InvalidAccount);
    }

    let nonce_bytes = rlp
        .at(0)
        .map_err(|_| CryptoError::InvalidAccount)?
        .data()
        .map_err(|_| CryptoError::InvalidAccount)?;
    if nonce_bytes.len() > 8 {
        return Err(CryptoError::InvalidAccount);
    }
    let mut nonce_buf = [0u8; 8];
    nonce_buf[8 - nonce_bytes.len()..].copy_from_slice(nonce_bytes);
    let nonce = u64::from_be_bytes(nonce_buf);

    let balance_bytes = rlp
        .at(1)
        .map_err(|_| CryptoError::InvalidAccount)?
        .data()
        .map_err(|_| CryptoError::InvalidAccount)?;
    if balance_bytes.len() > 16 {
        return Err(CryptoError::BalanceOverflow);
    }
    let mut balance_buf = [0u8; 16];
    balance_buf[16 - balance_bytes.len()..].copy_from_slice(balance_bytes);
    let balance = u128::from_be_bytes(balance_buf);

    let storage_root_bytes = rlp
        .at(2)
        .map_err(|_| CryptoError::InvalidAccount)?
        .data()
        .map_err(|_| CryptoError::InvalidAccount)?;
    if storage_root_bytes.len() != 32 {
        return Err(CryptoError::InvalidAccount);
    }
    let mut storage_root = [0u8; 32];
    storage_root.copy_from_slice(storage_root_bytes);

    let code_hash_bytes = rlp
        .at(3)
        .map_err(|_| CryptoError::InvalidAccount)?
        .data()
        .map_err(|_| CryptoError::InvalidAccount)?;
    if code_hash_bytes.len() != 32 {
        return Err(CryptoError::InvalidAccount);
    }
    let mut code_hash = [0u8; 32];
    code_hash.copy_from_slice(code_hash_bytes);

    Ok(AccountState {
        nonce,
        balance,
        storage_root,
        code_hash,
    })
}

fn verify_trie_proof(
    expected_root: [u8; 32],
    key: &[u8],
    proof: &[Vec<u8>],
) -> Result<Vec<u8>, CryptoError> {
    let mut nibbles = Vec::with_capacity(key.len() * 2);
    for byte in key {
        nibbles.push(byte >> 4);
        nibbles.push(byte & 0x0f);
    }
    verify_node(expected_root, &nibbles, proof, 0, 0)
}

fn verify_node(
    expected_root: [u8; 32],
    key: &[u8],
    proof: &[Vec<u8>],
    key_index: usize,
    proof_index: usize,
) -> Result<Vec<u8>, CryptoError> {
    let node = proof.get(proof_index).ok_or(CryptoError::InvalidProof)?;

    if key_index == 0 {
        if keccak256(node) != expected_root {
            return Err(CryptoError::InvalidProof);
        }
    } else if node.len() < 32 {
        if node.as_slice() != expected_root {
            return Err(CryptoError::InvalidProof);
        }
    } else if keccak256(node) != expected_root {
        return Err(CryptoError::InvalidProof);
    }

    let rlp = Rlp::new(node);
    let item_count = rlp.item_count().map_err(|_| CryptoError::InvalidProof)?;

    if item_count == 17 {
        if key_index >= key.len() {
            if proof_index + 1 != proof.len() {
                return Err(CryptoError::InvalidProof);
            }
            return rlp
                .at(16)
                .map_err(|_| CryptoError::InvalidProof)?
                .data()
                .map(|v| v.to_vec())
                .map_err(|_| CryptoError::InvalidProof);
        }
        let child = rlp
            .at(key[key_index] as usize)
            .map_err(|_| CryptoError::InvalidProof)?
            .data()
            .map_err(|_| CryptoError::InvalidProof)?;
        if child.is_empty() {
            return Ok(Vec::new());
        }
        let mut expected = [0u8; 32];
        if child.len() != 32 {
            return Err(CryptoError::InvalidProof);
        }
        expected.copy_from_slice(child);
        return verify_node(expected, key, proof, key_index + 1, proof_index + 1);
    }

    if item_count != 2 {
        return Err(CryptoError::InvalidProof);
    }

    let path_bytes = rlp
        .at(0)
        .map_err(|_| CryptoError::InvalidProof)?
        .data()
        .map_err(|_| CryptoError::InvalidProof)?;
    if path_bytes.is_empty() {
        return Err(CryptoError::InvalidProof);
    }
    let head = path_bytes[0] >> 4;
    if head > 3 {
        return Err(CryptoError::InvalidProof);
    }
    let is_leaf = head >= 2;
    let is_odd = head % 2 == 1;

    let mut path = Vec::with_capacity(path_bytes.len() * 2);
    if is_odd {
        path.push(path_bytes[0] & 0x0f);
    }
    for byte in &path_bytes[1..] {
        path.push(byte >> 4);
        path.push(byte & 0x0f);
    }

    if is_leaf {
        if key_index + path.len() != key.len() {
            return Ok(Vec::new());
        }
        if path.as_slice() != &key[key_index..] {
            return Ok(Vec::new());
        }
        if proof_index + 1 != proof.len() {
            return Err(CryptoError::InvalidProof);
        }
        return rlp
            .at(1)
            .map_err(|_| CryptoError::InvalidProof)?
            .data()
            .map(|v| v.to_vec())
            .map_err(|_| CryptoError::InvalidProof);
    }

    if key_index + path.len() > key.len() {
        return Ok(Vec::new());
    }
    if path.as_slice() != &key[key_index..key_index + path.len()] {
        return Ok(Vec::new());
    }
    let child = rlp
        .at(1)
        .map_err(|_| CryptoError::InvalidProof)?
        .data()
        .map_err(|_| CryptoError::InvalidProof)?;
    let mut expected = [0u8; 32];
    if child.len() != 32 {
        return Err(CryptoError::InvalidProof);
    }
    expected.copy_from_slice(child);
    verify_node(
        expected,
        key,
        proof,
        key_index + path.len(),
        proof_index + 1,
    )
}

pub fn address_from_derived_pubkey(pubkey: &str) -> Result<[u8; 20], CryptoError> {
    let raw = pubkey
        .strip_prefix("secp256k1:")
        .ok_or(CryptoError::InvalidPublicKey)?;
    let bytes = bs58::decode(raw)
        .into_vec()
        .map_err(|_| CryptoError::InvalidPublicKey)?;
    if bytes.len() != 64 {
        return Err(CryptoError::InvalidPublicKey);
    }
    let hash = keccak256(&bytes);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    Ok(address)
}

pub fn bytes_hex(value: &[u8]) -> String {
    format!("0x{}", hex::encode(value))
}

pub fn address_hex(address: &[u8; 20]) -> String {
    bytes_hex(address)
}

pub fn parse_address_hex(value: &str) -> Result<[u8; 20], CryptoError> {
    let raw = value
        .strip_prefix("0x")
        .ok_or(CryptoError::InvalidAccount)?;
    let bytes = hex::decode(raw).map_err(|_| CryptoError::InvalidAccount)?;
    if bytes.len() != 20 {
        return Err(CryptoError::InvalidAccount);
    }
    let mut address = [0u8; 20];
    address.copy_from_slice(&bytes);
    Ok(address)
}

pub fn recover_signer(
    prehash: &[u8; 32],
    r: &[u8; 32],
    s: &[u8; 32],
    recovery_id: u8,
) -> Result<([u8; 20], u8, [u8; 32]), CryptoError> {
    if recovery_id > 1 {
        return Err(CryptoError::InvalidSignature);
    }
    let mut signature =
        Signature::from_scalars(*r, *s).map_err(|_| CryptoError::InvalidSignature)?;
    let mut recid = RecoveryId::from_byte(recovery_id).ok_or(CryptoError::InvalidSignature)?;

    if let Some(normalized) = signature.normalize_s() {
        signature = normalized;
        recid = RecoveryId::from_byte(1 - recovery_id).ok_or(CryptoError::InvalidSignature)?;
    }

    let signature_bytes = signature.to_bytes();
    let mut normalized_s = [0u8; 32];
    normalized_s.copy_from_slice(&signature_bytes[32..]);

    let verifying_key = VerifyingKey::recover_from_prehash(prehash, &signature, recid)
        .map_err(|_| CryptoError::InvalidSignature)?;
    let point = verifying_key.to_encoded_point(false);
    let bytes = point.as_bytes();
    let hash = keccak256(&bytes[1..]);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    Ok((address, recid.to_byte(), normalized_s))
}

pub fn recover_address(
    prehash: &[u8; 32],
    r: &[u8; 32],
    s: &[u8; 32],
    recovery_id: u8,
) -> Result<[u8; 20], CryptoError> {
    recover_signer(prehash, r, s, recovery_id).map(|(address, _, _)| address)
}

fn append_u128(stream: &mut RlpStream, value: u128) {
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(32);
    stream.append(&bytes[first..].to_vec());
}

fn append_trimmed(stream: &mut RlpStream, bytes: &[u8]) {
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    stream.append(&bytes[first..].to_vec());
}

#[derive(Clone, Copy)]
pub struct TxParams<'a> {
    pub chain_id: u64,
    pub nonce: u64,
    pub max_priority_fee: u128,
    pub max_fee: u128,
    pub gas_limit: u64,
    pub to: [u8; 20],
    pub value: u128,
    pub data: &'a [u8],
}

fn append_tx_fields(stream: &mut RlpStream, params: &TxParams) {
    stream.append(&params.chain_id);
    stream.append(&params.nonce);
    append_u128(stream, params.max_priority_fee);
    append_u128(stream, params.max_fee);
    stream.append(&params.gas_limit);
    stream.append(&params.to.to_vec());
    append_u128(stream, params.value);
    stream.append(&params.data.to_vec());
    stream.append_raw(&[0xc0], 1);
}

pub fn eip1559_signing_hash(params: &TxParams) -> [u8; 32] {
    let mut stream = RlpStream::new_list(9);
    append_tx_fields(&mut stream, params);

    let encoded = stream.out();
    let mut payload = Vec::with_capacity(1 + encoded.len());
    payload.push(0x02);
    payload.extend_from_slice(&encoded);
    keccak256(&payload)
}

pub fn assemble_signed_tx(params: &TxParams, y_parity: u8, r: &[u8; 32], s: &[u8; 32]) -> Vec<u8> {
    let mut stream = RlpStream::new_list(12);
    append_tx_fields(&mut stream, params);
    stream.append(&(y_parity as u64));
    append_trimmed(&mut stream, r);
    append_trimmed(&mut stream, s);

    let encoded = stream.out();
    let mut payload = Vec::with_capacity(1 + encoded.len());
    payload.push(0x02);
    payload.extend_from_slice(&encoded);
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::Value;
    use std::sync::OnceLock;

    const VECTORS: &str = include_str!("../testdata/sepolia_12058306.json");
    const TX_VECTORS: &str = include_str!("../testdata/sepolia_tx_12058498.json");

    fn hex_bytes(value: &str) -> Vec<u8> {
        let raw = value.strip_prefix("0x").unwrap_or(value);
        hex::decode(raw).expect("valid hex")
    }

    fn vectors() -> &'static Value {
        static DATA: OnceLock<Value> = OnceLock::new();
        DATA.get_or_init(|| serde_json::from_str(VECTORS).unwrap())
    }

    fn header_rlp() -> Vec<u8> {
        hex_bytes(vectors()["headerRlp"].as_str().unwrap())
    }

    #[test]
    fn parses_captured_sepolia_header() {
        let data: Value = serde_json::from_str(VECTORS).unwrap();
        let header = hex_bytes(data["headerRlp"].as_str().unwrap());
        let fields = parse_header_fields(&header).unwrap();

        assert_eq!(fields.number, data["blockNumber"].as_u64().unwrap());
        assert_eq!(
            bytes_hex(&fields.state_root),
            data["stateRoot"].as_str().unwrap()
        );
        assert_eq!(
            bytes_hex(&keccak256(&header)),
            data["blockHash"].as_str().unwrap()
        );
    }

    #[test]
    fn verifies_eoa_account_proof() {
        let data: Value = serde_json::from_str(VECTORS).unwrap();
        let fields = parse_header_fields(&hex_bytes(data["headerRlp"].as_str().unwrap())).unwrap();
        let account = &data["accounts"]["0x0000000000000000000000000000000000000001"];
        let proof: Vec<Vec<u8>> = account["accountProof"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| hex_bytes(value.as_str().unwrap()))
            .collect();
        let address = parse_address_hex("0x0000000000000000000000000000000000000001").unwrap();

        let state = verify_account_proof(fields.state_root, address, &proof)
            .unwrap()
            .expect("account exists");
        assert!(state.is_eoa());
        assert_eq!(state.nonce, 0);
        assert_eq!(state.balance, 0x418c84296add147a);
    }

    #[test]
    fn recognizes_contract_account_proof() {
        let data: Value = serde_json::from_str(VECTORS).unwrap();
        let fields = parse_header_fields(&hex_bytes(data["headerRlp"].as_str().unwrap())).unwrap();
        let account = &data["accounts"]["0x1c7d4b196cb0c7b01d743fbc6116a902379c7238"];
        let proof: Vec<Vec<u8>> = account["accountProof"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| hex_bytes(value.as_str().unwrap()))
            .collect();
        let address = parse_address_hex("0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238").unwrap();

        let state = verify_account_proof(fields.state_root, address, &proof)
            .unwrap()
            .expect("account exists");
        assert!(!state.is_eoa());
    }

    #[test]
    fn verifies_absence_proof() {
        let data: Value = serde_json::from_str(VECTORS).unwrap();
        let fields = parse_header_fields(&hex_bytes(data["headerRlp"].as_str().unwrap())).unwrap();
        let account = &data["accounts"]["0xec29df4cf529cf8be4fbc8380cf732cff82389bc"];
        let proof: Vec<Vec<u8>> = account["accountProof"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| hex_bytes(value.as_str().unwrap()))
            .collect();
        let address = parse_address_hex("0xec29df4cf529cf8be4fbc8380cf732cff82389bc").unwrap();

        assert!(verify_account_proof(fields.state_root, address, &proof)
            .unwrap()
            .is_none());
    }

    #[test]
    fn rejects_tampered_proof() {
        let data: Value = serde_json::from_str(VECTORS).unwrap();
        let fields = parse_header_fields(&hex_bytes(data["headerRlp"].as_str().unwrap())).unwrap();
        let account = &data["accounts"]["0x0000000000000000000000000000000000000001"];
        let mut proof: Vec<Vec<u8>> = account["accountProof"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| hex_bytes(value.as_str().unwrap()))
            .collect();
        proof[0][0] ^= 0xff;
        let address = parse_address_hex("0x0000000000000000000000000000000000000001").unwrap();
        assert!(verify_account_proof(fields.state_root, address, &proof).is_err());
    }

    #[test]
    fn derives_known_deposit_address() {
        let pubkey = "secp256k1:4ha5b44BhvbGUgJvUBtenH8GLZesA2HxiaSjzjR2QfipnCZ8A2EuH2w8TZSLYecCuprDzHpohHLauTgU2XZwXRgn";
        assert_eq!(
            address_hex(&address_from_derived_pubkey(pubkey).unwrap()),
            "0xec29df4cf529cf8be4fbc8380cf732cff82389bc"
        );
    }

    #[test]
    fn recovers_signer_address() {
        use k256::ecdsa::SigningKey;

        let signing_key = SigningKey::from_slice(&[7u8; 32]).unwrap();
        let prehash = keccak256(b"native-credit test");
        let (signature, recovery_id) = signing_key.sign_prehash_recoverable(&prehash).unwrap();
        let bytes = signature.to_bytes();
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..]);

        let recovered = recover_address(&prehash, &r, &s, recovery_id.to_byte()).unwrap();
        let expected = {
            let point = signing_key.verifying_key().to_encoded_point(false);
            let hash = keccak256(&point.as_bytes()[1..]);
            let mut address = [0u8; 20];
            address.copy_from_slice(&hash[12..]);
            address
        };
        assert_eq!(recovered, expected);
    }

    #[test]
    fn eip1559_signing_hash_is_deterministic() {
        let to = parse_address_hex("0x0000000000000000000000000000000000000001").unwrap();
        let params = TxParams {
            chain_id: 11155111,
            nonce: 0,
            max_priority_fee: 1,
            max_fee: 2,
            gas_limit: 21000,
            to,
            value: 100,
            data: &[],
        };
        let hash = eip1559_signing_hash(&params);
        let hash2 = eip1559_signing_hash(&params);
        assert_eq!(hash, hash2);
    }
    #[test]
    fn reproduces_real_sepolia_eip1559_transaction() {
        let data: Value = serde_json::from_str(TX_VECTORS).unwrap();
        let to_bytes = hex_bytes(data["to"].as_str().unwrap());
        let mut to = [0u8; 20];
        to.copy_from_slice(&to_bytes);
        let r_bytes = hex_bytes(data["r"].as_str().unwrap());
        let s_bytes = hex_bytes(data["s"].as_str().unwrap());
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&r_bytes);
        s.copy_from_slice(&s_bytes);
        let parse_u128 = |key: &str| {
            u128::from_str_radix(data[key].as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
        };
        let parse_u64 = |key: &str| {
            u64::from_str_radix(data[key].as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
        };
        let y_parity =
            u8::from_str_radix(data["v"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();

        let params = TxParams {
            chain_id: parse_u64("chainId"),
            nonce: parse_u64("nonce"),
            max_priority_fee: parse_u128("maxPriorityFeePerGas"),
            max_fee: parse_u128("maxFeePerGas"),
            gas_limit: parse_u64("gas"),
            to,
            value: parse_u128("value"),
            data: &[],
        };
        let signing_hash = eip1559_signing_hash(&params);
        let (recovered, normalized_recovery_id, normalized_s) =
            recover_signer(&signing_hash, &r, &s, y_parity).unwrap();
        assert_eq!(address_hex(&recovered), data["from"].as_str().unwrap());
        assert_eq!(normalized_recovery_id, y_parity);
        assert_eq!(normalized_s, s);

        let raw = assemble_signed_tx(&params, normalized_recovery_id, &r, &normalized_s);
        assert_eq!(bytes_hex(&raw), data["raw"].as_str().unwrap());
        assert_eq!(bytes_hex(&keccak256(&raw)), data["hash"].as_str().unwrap());
    }

    #[test]
    fn measures_captured_proof_and_tx_payload_sizes() {
        let data = vectors();
        let tx: Value = serde_json::from_str(TX_VECTORS).unwrap();
        let header = header_rlp();
        let mut total_proof_bytes = 0usize;
        let mut max_node_bytes = 0usize;
        for (_, account) in data["accounts"].as_object().unwrap() {
            for node in account["accountProof"].as_array().unwrap() {
                let size = hex_bytes(node.as_str().unwrap()).len();
                total_proof_bytes += size;
                max_node_bytes = max_node_bytes.max(size);
            }
        }
        let raw_tx = hex_bytes(tx["raw"].as_str().unwrap()).len();
        println!(
            "header={}B total_account_proof={total_proof_bytes}B max_node={max_node_bytes}B raw_tx={raw_tx}B",
            header.len()
        );
        assert!(header.len() <= 2048);
        assert!(total_proof_bytes <= 16_384);
        assert!(max_node_bytes <= 1024);
        assert!(raw_tx <= 512);
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]

        #[test]
        fn header_parse_never_panics_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512),
        ) {
            let _ = parse_header_fields(&data);
        }

        #[test]
        fn truncated_sepolia_header_is_rejected(length in 0usize..header_rlp().len()) {
            let header = header_rlp();
            prop_assert!(parse_header_fields(&header[..length]).is_err());
        }

        #[test]
        fn account_decode_never_panics_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..256),
        ) {
            let _ = decode_account(&data);
        }

        #[test]
        fn malformed_trie_proof_never_panics(
            proof in proptest::collection::vec(
                proptest::collection::vec(any::<u8>(), 0..192),
                0..10,
            ),
        ) {
            let fields = parse_header_fields(&header_rlp()).unwrap();
            let address =
                parse_address_hex("0x0000000000000000000000000000000000000001").unwrap();
            let _ = verify_account_proof(fields.state_root, address, &proof);
        }

        #[test]
        fn minimal_rlp_u128_encoding_round_trips(value in any::<u128>()) {
            let mut stream = RlpStream::new_list(1);
            append_u128(&mut stream, value);
            let encoded = stream.out();
            let rlp = Rlp::new(&encoded);
            let payload = rlp.at(0).unwrap().data().unwrap();
            prop_assert!(payload.len() <= 16);
            let mut buf = [0u8; 16];
            buf[16 - payload.len()..].copy_from_slice(payload);
            prop_assert_eq!(u128::from_be_bytes(buf), value);
        }
    }
}
