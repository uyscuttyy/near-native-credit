import test from "node:test";
import assert from "node:assert/strict";
import { classifyPosition, renderTable } from "../src/keeper.mjs";

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
};

test("healthy position is not a liquidation candidate", () => {
  const row = classifyPosition({
    positionId: 1,
    position,
    health: "120000",
    price,
    config,
    nowSeconds: now,
  });
  assert.equal(row.liquidatable, false);
  assert.equal(row.near_liquidation, false);
  assert.equal(row.estimated_seized_wei, "0");
});

test("below-threshold health is liquidatable with an estimated seize", () => {
  const row = classifyPosition({
    positionId: 1,
    position,
    health: "9000",
    price,
    config,
    nowSeconds: now,
  });
  assert.equal(row.liquidatable, true);
  assert.equal(row.health_label, "0.90x");
  assert.equal(row.estimated_seized_wei, "3600000000000000");
});

test("near-threshold health is flagged but not liquidatable", () => {
  const row = classifyPosition({
    positionId: 1,
    position,
    health: "11000",
    price,
    config,
    nowSeconds: now,
  });
  assert.equal(row.liquidatable, false);
  assert.equal(row.near_liquidation, true);
});

test("stale prices suppress liquidation candidates", () => {
  const row = classifyPosition({
    positionId: 1,
    position,
    health: "9000",
    price: { ...price, timestamp: now - 301 },
    config,
    nowSeconds: now,
  });
  assert.equal(row.liquidatable, false);
  assert.equal(row.price_stale, true);
  assert.equal(row.estimated_seized_wei, "0");
});

test("missing positions are omitted", () => {
  const row = classifyPosition({
    positionId: 99,
    position: null,
    health: null,
    price,
    config,
    nowSeconds: now,
  });
  assert.equal(row, null);
});

test("debt-free positions are never liquidation candidates", () => {
  const row = classifyPosition({
    positionId: 1,
    position: { ...position, debt_usdc: "0" },
    health: "9000",
    price,
    config,
    nowSeconds: now,
  });
  assert.equal(row.liquidatable, false);
  assert.equal(row.estimated_seized_wei, "0");
});

test("table rendering includes the candidate state", () => {
  const text = renderTable([
    classifyPosition({
      positionId: 1,
      position,
      health: "9000",
      price,
      config,
      nowSeconds: now,
    }),
  ]);
  assert.match(text, /LIQUIDATABLE/);
});
