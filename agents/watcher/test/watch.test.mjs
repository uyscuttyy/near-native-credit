import test from "node:test";
import assert from "node:assert/strict";
import { keccak256 } from "ethereum-cryptography/keccak.js";
import {
  evaluateSnapshot,
  rawTxHashMatches,
  renderReport,
} from "../src/watch.mjs";

const now = 1_793_900_000;
const config = {
  max_oracle_age_secs: 300,
  risk_off: false,
};
const price = {
  eth_usd_1e8: "300000000000",
  usdc_usd_1e8: "100000000",
  timestamp: now - 20,
  sequence: 9,
};

function position(overrides = {}) {
  return {
    owner: "borrower.native-credit.testnet",
    status: "Idle",
    collateral_wei: "50000000000000000",
    debt_usdc: "0",
    pending_deposit: null,
    pending_withdrawal: null,
    pending_borrow: null,
    ...overrides,
  };
}

test("healthy position has no warnings or critical alerts", () => {
  const alerts = evaluateSnapshot({
    position: position(),
    health: "340282366920938463463374607431768211455",
    price,
    config,
    nowSeconds: now,
  });
  assert.equal(alerts.filter((alert) => alert.severity !== "info").length, 0);
});

test("liquidatable position raises a critical alert", () => {
  const alerts = evaluateSnapshot({
    position: position({ debt_usdc: "20000000" }),
    health: "9000",
    price,
    config,
    nowSeconds: now,
  });
  assert.ok(
    alerts.some(
      (alert) => alert.severity === "critical" && alert.code === "liquidatable",
    ),
  );
});

test("near-liquidation position raises a warning", () => {
  const alerts = evaluateSnapshot({
    position: position({ debt_usdc: "10000000" }),
    health: "11000",
    price,
    config,
    nowSeconds: now,
  });
  assert.ok(alerts.some((alert) => alert.code === "near_liquidation"));
});

test("stale and missing oracle prices are flagged", () => {
  const stale = evaluateSnapshot({
    position: position(),
    health: "120000",
    price: { ...price, timestamp: now - 301 },
    config,
    nowSeconds: now,
  });
  assert.ok(stale.some((alert) => alert.code === "oracle_stale"));

  const missing = evaluateSnapshot({
    position: position(),
    health: "120000",
    price: null,
    config,
    nowSeconds: now,
  });
  assert.ok(missing.some((alert) => alert.code === "oracle_missing"));
});

test("NeedsReview and risk-off are surfaced", () => {
  const alerts = evaluateSnapshot({
    position: position({ status: "NeedsReview" }),
    health: "120000",
    price,
    config: { ...config, risk_off: true },
    nowSeconds: now,
  });
  assert.ok(alerts.some((alert) => alert.severity === "critical"));
  assert.ok(alerts.some((alert) => alert.code === "risk_off"));
});

test("unsigned pending withdrawal is reported", () => {
  const alerts = evaluateSnapshot({
    position: position({
      status: "WithdrawalPending",
      pending_withdrawal: {
        amount_wei: "10000000000000000",
        raw_tx: null,
        tx_hash: null,
      },
    }),
    health: "120000",
    price,
    config,
    nowSeconds: now,
  });
  assert.ok(alerts.some((alert) => alert.code === "pending_withdrawal_unsigned"));
});

test("raw transaction hash is checked against stored bytes", () => {
  const raw = Uint8Array.from([2, 248, 1, 2, 3]);
  const hash = keccak256(raw);
  assert.equal(rawTxHashMatches({ raw_tx: Array.from(raw), tx_hash: Array.from(hash) }), true);
  assert.equal(
    rawTxHashMatches({ raw_tx: Array.from(raw), tx_hash: [1, 2, 3] }),
    false,
  );
});

test("a tampered raw transaction raises a critical alert", () => {
  const alerts = evaluateSnapshot({
    position: position({
      status: "WithdrawalPending",
      pending_withdrawal: {
        amount_wei: "10000000000000000",
        raw_tx: [1, 2, 3],
        tx_hash: [9, 9, 9],
      },
    }),
    health: "120000",
    price,
    config,
    nowSeconds: now,
  });
  assert.ok(alerts.some((alert) => alert.code === "raw_tx_hash_mismatch"));
});

test("a failed Sepolia receipt is a critical withdrawal alert", () => {
  const alerts = evaluateSnapshot({
    position: position({
      status: "WithdrawalPending",
      pending_withdrawal: {
        amount_wei: "10000000000000000",
        tx_hash: [1, 2, 3],
      },
    }),
    health: "120000",
    price,
    config,
    receipt: { status: "0x0", gasUsed: "0x5208" },
    nowSeconds: now,
  });
  assert.ok(
    alerts.some(
      (alert) => alert.severity === "critical" && alert.code === "withdrawal_failed",
    ),
  );
});

test("renderReport summarizes alerts", () => {
  const text = renderReport({
    positionId: 1,
    position: position(),
    health: "120000",
    alerts: [
      { severity: "warning", code: "oracle_stale", message: "stale" },
    ],
  });
  assert.match(text, /position 1/);
  assert.match(text, /oracle_stale/);
});
