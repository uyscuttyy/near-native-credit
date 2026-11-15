#!/usr/bin/env python3
import shutil
import subprocess
import sys
from pathlib import Path

LIB = "contracts/native-credit/src/lib.rs"

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


def run(command, cwd=None):
    return subprocess.run(command, cwd=cwd, text=True, capture_output=True)


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
            result = run(command)
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
    return 0


if __name__ == "__main__":
    sys.exit(main())
