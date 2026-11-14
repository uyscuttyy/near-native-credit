# Audit follow-up: live withdrawal and chain-id evidence

Date: 2026-11-06 (updated 2026-11-07)
Baseline: `audit-baseline` (`f8241dc`)
Scope: verify the audit's claims about the live position `1` withdrawal and the
deployed `chain_id`.

## 1. The audited withdrawal did not move ETH

The audit narrative says the withdrawal to the registered payout address
succeeded. Live Sepolia and NEAR state do not support that claim.

| Item | Value |
| --- | --- |
| Attempted Sepolia tx | `0x63e95e272ff6b4eb1f74c8ca814b0894a8d3f68eb7b845e546aea00319f298c2` |
| Included block | `12068212` |
| Receipt status | `0x0` (failed) |
| Gas used | `21000` |
| Recipient | `0x0d5d9acdb6adf6dce00d14972f7ce7b1e99b8fa6` |
| Recipient balance after | `0` |
| Position state | `Idle`, no pending withdrawal |
| Collateral after settlement | `49966999999448000` wei |
| `last_verified_nonce` | `2` |
| `last_verified_block` | `12078967` |

The collateral changed from `50000000000000000` to `49966999999448000`, a
reduction of `330000055520000` wei. That is consistent with gas only; no
`0.01 ETH` left the account.

The earlier withdrawal attempt,
`0x671d78716a1bcfc79931b78d2764d340cd85e866e2df902ad1ae9e8ba0b72362`, was a
self-transfer back to the deposit address. It moved no net ETH, so the contract
correctly treated the resulting balance delta as a failed withdrawal. The audit
should not present either transaction as a successful user withdrawal.

## 2. Root cause: fixed gas limits are too low for a new account

The v1 config had `gas_limit=21000`. A value transfer to an account that does
not yet exist needs at least 21,000 intrinsic gas plus new-account state gas.
The live v1 receipt used exactly 21,000 gas and failed.

v2 raised the limit to `60000`, but that is still insufficient on the current
Sepolia gas schedule. EIP-8037 charges `183600` state gas to create a new
account. v2's fresh-EOA withdrawal
`0x66a3a1e2e4cb7a5d70ec55ac10a0fea97255bf077035a04ef2396c70fd7fc74d`
failed at block `12080523` with `gasUsed=60000` and status `0x0`. A
`debug_traceCall` shows `executionGasUsed=21000`, `stateGasUsed=183600`, and
`eth_estimateGas` returns `207391`.

A transfer to the already-existing EOA `0xec29...89bc` succeeds within
`21000`; v2's
`0x0ce9ccdf7abacfd77b4d4569e46ea3dcefb512034a66d68b4e180b42d2d4ca40`
did so at block `12080627`. The distinction is account existence, not the
withdrawal amount.

The contract currently has no config-update method, so remediation requires one
of:

1. pre-create or pre-fund the already-registered payout address, then retry the
   withdrawal; or
2. register an already-funded Sepolia EOA as the payout address; or
3. redeploy with a larger fixed gas limit (for example 50,000 or 60,000) and
   update the init config and tests.

Option 3 was first attempted with `v2.native-credit.testnet` at
`gas_limit=60000`, which is still below the ~`207000` required for a fresh EOA
under EIP-8037. The v2 withdrawal to a fresh address therefore failed, while a
withdrawal to an existing EOA succeeded. The contract was then upgraded in
place: code deploy tx
`AoaLM6MDSS2nsziqgmmgwhmjpAA65KdmMGCv7gYGBoJT`, then owner-only `set_gas_limit`
tx `FF9QM6yfBFhHTheWvUuT5W9cJe4KStnPjvqUTHz9cRLp`. Live config is now
`gas_limit=250000`, and the upgraded code rejects `60000` in `new` and
`set_gas_limit`. A fresh-address retry is pending a top-up above the `0.01 ETH`
minimum withdrawal. The retired v1 contract should not be used.

## 3. The chain-id finding is not reproducible

Live `get_config` returns `chain_id=11155111`, which is the correct Sepolia
chain id. The audit's claim that the contract is configured with a different
chain id could not be reproduced. Re-check the audit's decoding path before
carrying that finding forward.

## 4. Agent coverage added after the audit

The new `agents/watcher` checks stored withdrawal raw transactions against their
stored `tx_hash` and flags mismatches. When a withdrawal is pending, it also
reads the Sepolia receipt and raises a critical alert for status `0x0`. It
reports pending withdrawals, stale or missing oracle prices, `NeedsReview`, and
near/liquidatable health.
`agents/keeper` ranks liquidation candidates, and `agents/liquidator` plans a
USDC `ft_transfer_call` liquidation with a dry run by default.

The watcher can detect unsigned or tampered transactions, but it cannot prove an
Ethereum transfer succeeded until finality confirmation. The failed
`0x63e95...` transaction is therefore a reminder to verify the receipt and
balance delta, not only the broadcast.

## Required document corrections

- Do not describe `0x671d...` or `0x63e95...` as a completed ETH withdrawal.
- State that position `1` settled as failed and collateral only fell by gas.
- Do not repeat the audit's chain-id mismatch without a fresh reproduction.
- Keep the v1 `21000` and original v2 `60000` failures visible as historical
  evidence; live v2 is now `250000`, but the fresh-address retry is still pending.
