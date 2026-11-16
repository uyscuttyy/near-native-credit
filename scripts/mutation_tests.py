#!/usr/bin/env python3
import shutil
import subprocess
import sys
from pathlib import Path

LIB = "contracts/native-credit/src/lib.rs"
MARKET_LIB = "contracts/market/src/lib.rs"
MARKET_MANIFEST = "contracts/market/Cargo.toml"

MARKET_MUTATIONS = [
    (
        "register owner guard removed",
        "non_owner_cannot_register_asset",
        'require!(env::predecessor_account_id() == self.owner, "owner only");',
        'require!(true, "owner only");',
        1,
    ),
    (
        "duplicate asset check removed",
        "register_asset_rejects_duplicate_on_chain",
        'require!(self.asset_keys.insert(key), "asset already exists");',
        'require!(true, "asset already exists");',
        1,
    ),
    (
        "supply-cap check removed",
        "deposit_respects_minimum_and_cap",
        ".is_some_and(|total| total <= entry.config.supply_cap.0),",
        ".is_some_and(|_| true),",
        1,
    ),
    (
        "min-deposit check removed",
        "deposit_respects_minimum_and_cap",
        '            amount >= entry.config.min_deposit.0,\n            "deposit below minimum"',
        '            true,\n            "deposit below minimum"',
        1,
    ),
    (
        "withdrawal rounding floor mutant",
        "ceil_rounding_never_pays_more_than_shares",
        "let shares_out = mul_div_ceil(amount.0, INDEX_SCALE, entry.pool.interest_index.0);",
        "let shares_out = mul_div(amount.0, INDEX_SCALE, entry.pool.interest_index.0);",
        1,
    ),
    (
        "withdraw liquidity check removed",
        "withdraw_requires_liquidity",
        '            amount.0\n                <= entry\n                    .pool\n                    .total_supply\n                    .0\n                    .saturating_sub(entry.pool.total_borrow.0),\n            "insufficient liquidity, queue withdrawal instead"',
        '            true,\n            "insufficient liquidity, queue withdrawal instead"',
        1,
    ),
    (
        "claim liquidity check removed",
        "claim_requires_liquidity_and_keeps_queue",
        '            payable\n                <= entry\n                    .pool\n                    .total_supply\n                    .0\n                    .saturating_sub(entry.pool.total_borrow.0),\n            "insufficient liquidity"',
        '            true,\n            "insufficient liquidity"',
        1,
    ),
    (
        "callback-only guard removed",
        "forged_success_cannot_clear_owed",
        '            env::predecessor_account_id() == env::current_account_id(),\n            "internal only"',
        '            true,\n            "internal only"',
        1,
    ),
    (
        "timelock elapsed check removed",
        "timelock_blocks_early_confirmation_then_applies",
        '            now_secs() >= pending.execute_at_secs,\n            "timelock not elapsed"',
        '            true,\n            "timelock not elapsed"',
        1,
    ),
    (
        "index growth removed",
        "accrual_splits_interest_between_suppliers_and_reserves",
        "let supplier_cut = interest - reserve_cut;",
        "let supplier_cut = 0;",
        1,
    ),    (
        "create-position storage deposit removed",
        "create_position_requires_storage_deposit_and_collateral_asset",
        'require!(\n            env::attached_deposit() >= NearToken::from_yoctonear(STORAGE_DEPOSIT_YOCTO),\n            "storage deposit required"\n        );',
        'require!(true, "storage deposit required");',
        1,
    ),
    (
        "collateral-asset check removed",
        "create_position_requires_storage_deposit_and_collateral_asset",
        'require!(entry.config.collateral_enabled, "asset is not a collateral asset");',
        'require!(true, "asset is not a collateral asset");',
        1,
    ),
    (
        "repay owner guard removed",
        "borrow_and_repay_round_trip",
        'require!(position.owner == sender_id, "repay owner only");',
        'require!(true, "repay owner only");',
        1,
    ),
    (
        "borrow health-check removed",
        "borrow_rejects_over_limit_cap_and_liquidity",
        'require!(new_debt_usd <= limit, "borrow limit exceeded");',
        'require!(true, "borrow limit exceeded");',
        1,
    ),
    (
        "borrow min-check removed",
        "borrow_rejects_over_limit_cap_and_liquidity",
        'require!(amount.0 >= entry.config.min_borrow.0, "below minimum");',
        'require!(true, "below minimum");',
        1,
    ),
    (
        "borrow liquidity check removed",
        "borrow_rejects_over_limit_cap_and_liquidity",
        'require!(\n            amount.0 <= entry.pool.total_supply.0.saturating_sub(entry.pool.total_borrow.0),\n            "insufficient liquidity"\n        );',
        'require!(true, "insufficient liquidity");',
        1,
    ),
    (
        "stale-price check removed",
        "borrow_requires_fresh_price",
        'require!(\n            price.timestamp.saturating_add(self.max_oracle_age_secs) >= now,\n            "stale price"\n        );',
        'require!(true, "stale price");',
        1,
    ),
    (
        "oracle threshold check removed",
        "update_price_rejects_bad_quorum_and_old_sequence",
        'require!(signatures.len() as u32 >= self.oracle_threshold, "oracle quorum not met");',
        'require!(true, "oracle quorum not met");',
        1,
    ),
    (
        "price sequence check removed",
        "update_price_rejects_bad_quorum_and_old_sequence",
        'require!(price.sequence > last.sequence, "price sequence too old");',
        'require!(true, "price sequence too old");',
        1,
    ),
    (
        "repay clamp removed",
        "borrow_and_repay_round_trip",
        "let accepted = remaining.min(debt.amount.0);",
        "let accepted = remaining;",
        1,
    ),
]

MUTATIONS = [
    (
        "chain-id validation removed",
        "init_rejects_sepolia_chain_id_typo",
        "lib",
        'require!(config.chain_id == SEPOLIA_CHAIN_ID, "invalid chain id");',
        'require!(true, "invalid chain id");',
    ),
    (
        "duplicate-oracle-key check removed",
        "init_rejects_duplicate_oracle_keys",
        "lib",
        """require!(
                    previous.as_bytes() != key.as_bytes(),
                    "duplicate oracle key"
                );""",
        'require!(true, "duplicate oracle key");',
    ),
    (
        "future-price check removed",
        "update_price_rejects_future_timestamp",
        "lib",
        '''            "invalid price"
        );
        let now = env::block_timestamp() / 1_000_000_000;
        require!(price.timestamp <= now, "future price");''',
        '''            "invalid price"
        );
        let now = env::block_timestamp() / 1_000_000_000;
        require!(true, "future price");''',
    ),
    (
        "stale-health check removed",
        "stale_price_is_not_reported_as_healthy",
        "lib",
        """if price.timestamp > now
            || price
                .timestamp
                .saturating_add(self.config.max_oracle_age_secs)
                < now
        {
            return U128(0);
        }""",
        """if false {
            return U128(0);
        }""",
    ),
    (
        "storage-deposit check removed",
        "rejects_missing_storage_deposit",
        "lib",
        "env::attached_deposit() >= NearToken::from_yoctonear(STORAGE_DEPOSIT_YOCTO),",
        "true,",
    ),
    (
        "terminal-ticket guard removed",
        "liquidation_signature_does_not_overwrite_terminal_ticket",
        "lib",
        "                .map(|ticket| ticket.status == TicketStatus::PendingSignature)",
        "                .map(|_| true)",
    ),
    (
        "set_gas_limit owner guard removed",
        "non_owner_cannot_set_gas_limit",
        "lib",
        '''    pub fn set_gas_limit(&mut self, gas_limit: u64) {
        require!(
            env::predecessor_account_id() == self.config.owner,
            "owner only"
        );''',
        '''    pub fn set_gas_limit(&mut self, gas_limit: u64) {
        require!(
            true,
            "owner only"
        );''',
    ),
    (
        "set_gas_limit headroom check removed",
        "gas_limit_cannot_exceed_min_withdrawal_headroom",
        "lib",
        "            gas_limit >= MIN_WITHDRAWAL_GAS_LIMIT && self.config.min_withdrawal_wei.0 > max_gas_wei,",
        "            gas_limit >= MIN_WITHDRAWAL_GAS_LIMIT && true,",
    ),
    (
        "signed-payout reconciliation guard removed",
        "local_e2e_proof_mismatch_requires_reconciliation_for_signed_payout",
        "e2e",
        'require!(withdrawal.raw_tx.is_none(), "signed payout requires proof");',
        'require!(true, "signed payout requires proof");',
    ),
]


def run_market_mutation(repo, worktree, current_head, mutation):
    description, test, old, new, _count = mutation
    target = worktree / MARKET_LIB
    manifest = worktree / MARKET_MANIFEST
    source = target.read_text()
    if source.count(old) == 0:
        print(f"[setup-error] {description}: pattern count 0")
        return False
    target.write_text(source.replace(old, new, 1))
    command = ["cargo", "test", "--manifest-path", str(manifest), "--lib", test]
    result = run(command, env={"CARGO_TARGET_DIR": str(repo / "target")})
    run(["git", "checkout", "--", MARKET_LIB], cwd=worktree)
    killed = result.returncode != 0
    print(f"[killed] {description}" if killed else f"[ALIVE] {description}")
    return killed


def run_market_mutations(repo, current_head):
    worktree = Path("/tmp/market-mutation")
    if not ensure_worktree(repo, worktree, current_head):
        return False
    survivors = []
    for mutation in MARKET_MUTATIONS:
        if not run_market_mutation(repo, worktree, current_head, mutation):
            survivors.append(mutation[0])
    run(["git", "checkout", "--", MARKET_LIB], cwd=worktree)
    if survivors:
        print(f"market mutation tests failed: {len(survivors)} survivor(s): {survivors}")
        return False
    print(f"market mutation tests passed: all {len(MARKET_MUTATIONS)} mutants killed")
    return True


def run(command, cwd=None, env=None):
    import os
    merged = os.environ.copy()
    if env:
        merged.update(env)
    return subprocess.run(command, cwd=cwd, text=True, capture_output=True, env=merged)


def ensure_worktree(repo, worktree, current_head):
    lib_clean = False
    head = ""
    if worktree.exists():
        head = run(["git", "rev-parse", "HEAD"], cwd=worktree).stdout.strip()
        lib_clean = (
            run(["git", "status", "--porcelain", "--", LIB], cwd=worktree).stdout.strip() == ""
        )
    if worktree.exists() and head == current_head and lib_clean:
        print(f"reusing worktree {worktree}")
        return True
    if worktree.exists():
        run(["git", "worktree", "remove", "--force", str(worktree)], cwd=repo)
    shutil.rmtree(worktree, ignore_errors=True)
    created = run(["git", "worktree", "add", "--detach", str(worktree), current_head], cwd=repo)
    if created.returncode != 0:
        print(created.stderr.strip())
        return False
    return True


def main():
    repo = Path(__file__).resolve().parent.parent
    worktree = Path("/tmp/native-credit-mutation")
    manifest = worktree / "contracts/native-credit/Cargo.toml"
    target = worktree / LIB
    current_head = run(["git", "rev-parse", "HEAD"], cwd=repo).stdout.strip()
    if not ensure_worktree(repo, worktree, current_head):
        return 1

    survivors = []
    try:
        for description, test, mode, old, new in MUTATIONS:
            source = target.read_text()
            if source.count(old) != 1:
                print(f"[setup-error] {description}: pattern not found")
                survivors.append(description)
                continue
            target.write_text(source.replace(old, new, 1))

            if mode == "e2e":
                command = ["cargo", "test", "--manifest-path", str(manifest), "--test", "local_e2e", test]
            else:
                command = ["cargo", "test", "--manifest-path", str(manifest), "--lib", test]
            result = run(command, env={"CARGO_TARGET_DIR": str(repo / "target")})
            killed = result.returncode != 0
            if killed:
                print(f"[killed] {description}")
            else:
                print(f"[ALIVE] {description}")
                survivors.append(description)
            run(["git", "checkout", "--", LIB], cwd=worktree)
    finally:
        run(["git", "checkout", "--", LIB], cwd=worktree)

    print()
    if survivors:
        print(f"mutation tests failed: {len(survivors)} survivor(s): {survivors}")
        return 1
    print(f"mutation tests passed: all {len(MUTATIONS)} mutants killed")
    if not run_market_mutations(repo, current_head):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
