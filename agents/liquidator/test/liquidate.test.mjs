import test from "node:test";
import assert from "node:assert/strict";
import {
  buildLiquidateCommand,
  buildRegisterCommand,
  displayCommand,
  planLiquidation,
} from "../src/liquidate.mjs";

const now = 1_793_900_000;
const config = {
  max_oracle_age_secs: 300,
  liquidation_penalty_bps: 800,
  risk_off: false,
};
const price = {
  eth_usd_1e8: "300000000000",
  usdc_usd_1e8: "100000000",
  timestamp: now - 20,
  sequence: 9,
};
const position = {
  owner: "borrower.native-credit.testnet",
  status: "Idle",
  collateral_wei: "50000000000000000",
  debt_usdc: "10000000",
  pending_deposit: null,
  pending_withdrawal: null,
  pending_borrow: null,
};

function plan(overrides = {}) {
  return planLiquidation({
    positionId: 1,
    position,
    health: "9000",
    price,
    config,
    balance: "10000000",
    nowSeconds: now,
    ...overrides,
  });
}

test("healthy positions cannot be liquidated", () => {
  const result = plan({ health: "120000" });
  assert.equal(result.ok, false);
  assert.match(result.reason, /not below/);
});

test("eligible position plans a full-debt liquidation", () => {
  const result = plan();
  assert.equal(result.ok, true);
  assert.equal(result.repay_usdc, "10000000");
  assert.equal(result.estimated_seized_wei, "3600000000000000");
  assert.equal(result.partial, false);
});

test("stale prices block liquidation", () => {
  const result = plan({ price: { ...price, timestamp: now - 301 } });
  assert.equal(result.ok, false);
  assert.match(result.reason, /stale/);
});

test("pending operations block liquidation", () => {
  const result = plan({
    position: { ...position, status: "WithdrawalPending" },
  });
  assert.equal(result.ok, false);
  assert.match(result.reason, /status/);
});

test("amounts above debt are rejected", () => {
  const result = plan({ amount: "10000001" });
  assert.equal(result.ok, false);
  assert.match(result.reason, /exceeds/);
});

test("partial mode caps repayment at the USDC balance", () => {
  const result = plan({ balance: "4000000", allowPartial: true });
  assert.equal(result.ok, true);
  assert.equal(result.repay_usdc, "4000000");
  assert.equal(result.partial, true);
});

test("insufficient balance without partial mode is rejected", () => {
  const result = plan({ balance: "4000000" });
  assert.equal(result.ok, false);
  assert.match(result.reason, /balance/);
});

test("liquidation command wraps liquidate intent in ft_transfer_call", () => {
  const built = buildLiquidateCommand({
    usdcId: "usdc.testnet",
    contractId: "native-credit.testnet",
    positionId: 7,
    amount: "12345",
    signer: "liquidator.testnet",
  });
  assert.equal(built.command, "near");
  assert.equal(built.args[3], "usdc.testnet");
  assert.equal(built.args[4], "ft_transfer_call");
  const outer = JSON.parse(built.args[6]);
  assert.equal(outer.receiver_id, "native-credit.testnet");
  assert.equal(outer.amount, "12345");
  assert.deepEqual(JSON.parse(outer.msg), {
    action: "liquidate",
    position_id: 7,
  });
  assert.equal(built.args[10], "1");
});

test("registration command calls register_sepolia_address", () => {
  const built = buildRegisterCommand({
    contractId: "native-credit.testnet",
    address: "0xabc",
    signer: "liquidator.testnet",
  });
  assert.equal(built.args[3], "native-credit.testnet");
  assert.equal(built.args[4], "register_sepolia_address");
  assert.deepEqual(JSON.parse(built.args[6]), { address: "0xabc" });
});

test("displayCommand quotes JSON arguments", () => {
  const built = buildLiquidateCommand({
    usdcId: "usdc.testnet",
    contractId: "native-credit.testnet",
    positionId: 1,
    amount: "1",
    signer: "liquidator.testnet",
  });
  const text = displayCommand(built);
  assert.match(text, /ft_transfer_call/);
  assert.match(text, /position_id/);
});
