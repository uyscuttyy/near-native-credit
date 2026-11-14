export const BPS = 10_000n;
export const LIQUIDATION_THRESHOLD_BPS = 10_000n;

export function toBigInt(value) {
  if (value === undefined || value === null) return null;
  return BigInt(value);
}

export function healthValue(health) {
  return toBigInt(health);
}

export function isLiquidatable(health) {
  const value = healthValue(health);
  return value !== null && value < LIQUIDATION_THRESHOLD_BPS;
}

export function isNearLiquidation(health, warningBps = 12_000n) {
  const value = healthValue(health);
  return value !== null && value >= LIQUIDATION_THRESHOLD_BPS && value < warningBps;
}

export function priceAgeSeconds(price, nowSeconds = Math.floor(Date.now() / 1000)) {
  if (!price?.timestamp) return null;
  return Math.max(0, nowSeconds - Number(price.timestamp));
}

export function isPriceStale(price, config, nowSeconds) {
  if (!price?.timestamp) return true;
  const age = priceAgeSeconds(price, nowSeconds);
  return age > Number(config?.max_oracle_age_secs ?? 300);
}

export function formatHealth(health) {
  const value = healthValue(health);
  if (value === null) return "—";
  if (value >= 2n ** 128n - 1n) return "∞";
  return `${(Number(value) / 10_000).toFixed(2)}x`;
}

export function evaluatePosition({
  position,
  health,
  price,
  config,
  nowSeconds = Math.floor(Date.now() / 1000),
}) {
  const alerts = [];
  const push = (severity, code, message) =>
    alerts.push({ severity, code, message });

  if (position.status === "NeedsReview") {
    push(
      "critical",
      "needs_review",
      "position is in NeedsReview; owner intervention required",
    );
  }

  if (isLiquidatable(health)) {
    push(
      "critical",
      "liquidatable",
      `health ${formatHealth(health)} is below 1.00x; liquidation is possible`,
    );
  } else if (isNearLiquidation(health)) {
    push(
      "warning",
      "near_liquidation",
      `health ${formatHealth(health)} is close to the 1.00x liquidation line`,
    );
  }

  if (!price) {
    push("warning", "oracle_missing", "no oracle price is available");
  } else if (isPriceStale(price, config, nowSeconds)) {
    push(
      "warning",
      "oracle_stale",
      `oracle is ${priceAgeSeconds(price, nowSeconds)}s old; new risk is paused`,
    );
  }

  if (config?.risk_off) {
    push("warning", "risk_off", "risk-off is active; borrow and withdraw are paused");
  }

  if (position.pending_deposit) {
    push(
      "warning",
      "pending_deposit",
      `deposit proof at block ${position.pending_deposit.header_number} is awaiting finality`,
    );
  }
  if (position.pending_borrow) {
    push(
      "warning",
      "pending_borrow",
      `borrow of ${position.pending_borrow} USDC base units is awaiting USDC transfer`,
    );
  }
  if (position.pending_withdrawal) {
    const withdrawal = position.pending_withdrawal;
    if (!withdrawal.raw_tx) {
      push(
        "warning",
        "pending_withdrawal_unsigned",
        `withdrawal of ${withdrawal.amount_wei} wei is awaiting an MPC signature`,
      );
    } else {
      push(
        "info",
        "pending_withdrawal_signed",
        `withdrawal of ${withdrawal.amount_wei} wei is signed and awaiting broadcast/confirmation`,
      );
    }
  }

  if (
    position.status === "Idle" &&
    BigInt(position.debt_usdc ?? "0") > 0n &&
    !position.pending_deposit &&
    !position.pending_withdrawal &&
    !position.pending_borrow
  ) {
    push(
      "info",
      "active_loan",
      `active debt ${position.debt_usdc} USDC, health ${formatHealth(health)}`,
    );
  }

  return alerts;
}

export function summarize(alerts) {
  const counts = { critical: 0, warning: 0, info: 0 };
  for (const alert of alerts) {
    counts[alert.severity] += 1;
  }
  return counts;
}

export function estimateSeizedWei({
  repayUsdc,
  usdcPrice1e8,
  ethPrice1e8,
  penaltyBps,
  collateralWei,
}) {
  if (repayUsdc <= 0n || ethPrice1e8 <= 0n) return 0n;
  const repayUsd1e8 = (repayUsdc * usdcPrice1e8) / 1_000_000n;
  const seizedWei =
    (repayUsd1e8 * (BPS + BigInt(penaltyBps))) / BPS * 10n ** 18n / ethPrice1e8;
  return seizedWei > collateralWei ? collateralWei : seizedWei;
}
