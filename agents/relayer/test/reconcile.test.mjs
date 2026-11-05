import assert from "node:assert/strict";
import test from "node:test";
import { isSettled, positionState } from "../src/reconcile.mjs";

function position(overrides = {}) {
  return {
    status: "Idle",
    pending_deposit: null,
    pending_withdrawal: null,
    pending_borrow: null,
    collateral_wei: "1000",
    debt_usdc: "0",
    last_verified_block: 10,
    last_verified_nonce: 2,
    ...overrides,
  };
}

test("reports a settled idle position", () => {
  assert.equal(isSettled(position()), true);
  assert.deepEqual(positionState(position()), {
    status: "Idle",
    pending: [],
    collateral: "1000",
    debt: "0",
    last_verified_block: 10,
    last_verified_nonce: 2,
  });
});

test("does not settle pending work", () => {
  assert.equal(isSettled(position({ pending_deposit: {} })), false);
  assert.equal(isSettled(position({ pending_withdrawal: {} })), false);
  assert.equal(isSettled(position({ pending_borrow: "10" })), false);
  assert.equal(
    isSettled(position({ status: "WithdrawalPending", pending_withdrawal: {} })),
    false,
  );
});

test("NeedsReview is terminal but not settled", () => {
  assert.equal(isSettled(position({ status: "NeedsReview" })), false);
  assert.equal(
    positionState(position({ status: "NeedsReview" })).status,
    "NeedsReview",
  );
});
