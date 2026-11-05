import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { setupWalletSelector } from "@near-wallet-selector/core";
import { setupMyNearWallet } from "@near-wallet-selector/my-near-wallet";
import { setupModal } from "@near-wallet-selector/modal-ui";
import "@near-wallet-selector/modal-ui/styles.css";
import {
  CONTRACT_ID,
  NEAR_RPC,
  USDC_ID,
  formatUnits,
  parseUnits,
  shortAddress,
  view,
} from "./near.js";

const MAX_U128 = 2n ** 128n - 1n;
const DEPOSIT_TX = "AEjzsP4swSxcvQ34ECAdpZMKiaZaHH6viqfckuvnyUgQ";
const REPAY_TX = "CW8wW3p1QvakoXQDDcx8jgUyztaf4daFGFmL2b6awHLo";
const WITHDRAW_TX =
  "0x671d78716a1bcfc79931b78d2764d340cd85e866e2df902ad1ae9e8ba0b72362";

function friendlyError(error) {
  const message = error?.message ?? String(error);
  if (/User rejected|rejected/.test(message)) return "Transaction rejected";
  return message;
}

function healthLabel(health) {
  if (health === undefined || health === null) return "—";
  const value = BigInt(health);
  if (value === MAX_U128) return "∞";
  return `${(Number(value) / 10000).toLocaleString(undefined, {
    maximumFractionDigits: 2,
  })}x`;
}

function statusTone(status) {
  if (!status) return "muted";
  if (status.includes("Pending")) return "warn";
  if (status === "NeedsReview") return "danger";
  if (status === "Deriving") return "muted";
  return "ok";
}

function Stat({ label, value, hint, tone }) {
  return (
    <div className={`stat ${tone ?? ""}`}>
      <span className="stat-label">{label}</span>
      <span className="stat-value">{value}</span>
      {hint ? <span className="stat-hint">{hint}</span> : null}
    </div>
  );
}

function Badge({ children, tone = "muted" }) {
  return <span className={`badge ${tone}`}>{children}</span>;
}

function CopyButton({ value, label = "Copy" }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="copy"
      onClick={async () => {
        await navigator.clipboard.writeText(value);
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      }}
    >
      {copied ? "Copied" : label}
    </button>
  );
}

function Link({ href, children }) {
  return (
    <a href={href} target="_blank" rel="noreferrer">
      {children}
    </a>
  );
}

export default function App() {
  const [positionId, setPositionId] = useState(1);
  const [config, setConfig] = useState(null);
  const [position, setPosition] = useState(null);
  const [price, setPrice] = useState(null);
  const [health, setHealth] = useState(null);
  const [usdcBalance, setUsdcBalance] = useState(null);
  const [storageBalance, setStorageBalance] = useState(null);
  const [now, setNow] = useState(Date.now());
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(null);
  const [error, setError] = useState(null);
  const [notice, setNotice] = useState(null);
  const [selector, setSelector] = useState(null);
  const [modal, setModal] = useState(null);
  const [accountId, setAccountId] = useState(null);
  const [borrowAmount, setBorrowAmount] = useState("10");
  const [repayAmount, setRepayAmount] = useState("10");
  const [withdrawAmount, setWithdrawAmount] = useState("0.01");
  const [sepoliaAddress, setSepoliaAddress] = useState("");
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    setupWalletSelector({
      network: "testnet",
      modules: [setupMyNearWallet()],
    })
      .then((nextSelector) => {
        if (cancelled) return;
        setSelector(nextSelector);
        setModal(
          setupModal(nextSelector, {
            contractId: CONTRACT_ID,
            description: "Connect to manage a Native Credit position on testnet.",
          }),
        );
        const sync = () => {
          const account = nextSelector.store.getState().accounts?.[0];
          setAccountId(account?.accountId ?? null);
        };
        sync();
        nextSelector.on("signedIn", sync);
        nextSelector.on("signedOut", sync);
        nextSelector.on("accountsChanged", sync);
        nextSelector.subscribeOnAccountChange((id) => setAccountId(id || null));
      })
      .catch((walletError) => {
        if (!cancelled) setError(walletError.message);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const [nextConfig, nextPrice, nextPosition, nextHealth] =
        await Promise.all([
          view(CONTRACT_ID, "get_config"),
          view(CONTRACT_ID, "get_price"),
          view(CONTRACT_ID, "get_position", { position_id: positionId }),
          view(CONTRACT_ID, "get_health", { position_id: positionId }),
        ]);
      if (!mounted.current) return;
      setConfig(nextConfig);
      setPrice(nextPrice);
      setPosition(nextPosition);
      setHealth(nextHealth);
      setNow(Date.now());
      if (accountId) {
        const [balance, storage] = await Promise.all([
          view(USDC_ID, "ft_balance_of", { account_id: accountId }),
          view(USDC_ID, "storage_balance_of", { account_id: accountId }),
        ]);
        if (!mounted.current) return;
        setUsdcBalance(balance);
        setStorageBalance(storage);
      }
    } catch (refreshError) {
      if (mounted.current) setError(refreshError.message);
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [accountId, positionId]);

  useEffect(() => {
    setLoading(true);
    refresh();
  }, [refresh]);

  useEffect(() => {
    const timer = setInterval(() => refresh(), 15000);
    const clock = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      clearInterval(timer);
      clearInterval(clock);
    };
  }, [refresh]);

  const priceAge = useMemo(() => {
    if (!price?.timestamp) return null;
    return Math.max(0, Math.floor(now / 1000) - Number(price.timestamp));
  }, [price, now]);

  const priceStale =
    priceAge === null || priceAge > Number(config?.max_oracle_age_secs ?? 300);
  const isOwner = Boolean(
    accountId && position?.owner && accountId === position.owner,
  );
  const hasPending = Boolean(
    position?.pending_deposit ||
      position?.pending_withdrawal ||
      position?.pending_borrow,
  );
  const canBorrow = isOwner && !hasPending && !priceStale && !config?.risk_off;
  const canWithdraw = isOwner && !hasPending && !priceStale && !config?.risk_off;
  const canRepay = isOwner && (position?.debt_usdc ?? "0") !== "0";

  const run = async (label, action) => {
    if (!selector) return;
    setBusy(label);
    setError(null);
    setNotice(null);
    try {
      const wallet = await selector.wallet();
      await action(wallet);
      setNotice(`${label} submitted`);
      setTimeout(() => refresh(), 2500);
    } catch (actionError) {
      setError(friendlyError(actionError));
    } finally {
      setBusy(null);
    }
  };

  const callContract = (wallet, methodName, args, gas = "100000000000000") =>
    wallet.signAndSendTransaction({
      receiverId: CONTRACT_ID,
      actions: [
        {
          type: "FunctionCall",
          params: {
            methodName,
            args,
            gas,
            deposit: "0",
          },
        },
      ],
    });

  const onBorrow = () =>
    run("Borrow", async (wallet) => {
      const amount = parseUnits(borrowAmount, 6);
      await callContract(wallet, "borrow", { position_id: positionId, amount });
    });

  const onWithdraw = () =>
    run("Withdraw", async (wallet) => {
      const amount = parseUnits(withdrawAmount, 18);
      await callContract(wallet, "withdraw", {
        position_id: positionId,
        amount,
      });
    });

  const onRepay = () =>
    run("Repay", async (wallet) => {
      const amount = parseUnits(repayAmount, 6);
      await wallet.signAndSendTransaction({
        receiverId: USDC_ID,
        actions: [
          {
            type: "FunctionCall",
            params: {
              methodName: "ft_transfer_call",
              args: {
                receiver_id: CONTRACT_ID,
                amount,
                msg: JSON.stringify({
                  action: "repay",
                  position_id: positionId,
                }),
              },
              gas: "150000000000000",
              deposit: "1",
            },
          },
        ],
      });
    });

  const onRegisterAddress = () =>
    run("Register address", async (wallet) => {
      await callContract(wallet, "register_sepolia_address", {
        address: sepoliaAddress.trim(),
      });
    });

  const onStorage = () =>
    run("USDC storage", async (wallet) => {
      await wallet.signAndSendTransaction({
        receiverId: USDC_ID,
        actions: [
          {
            type: "FunctionCall",
            params: {
              methodName: "storage_deposit",
              args: {},
              gas: "30000000000000",
              deposit: "1250000000000000000000",
            },
          },
        ],
      });
    });

  const onSignOut = async () => {
    const wallet = await selector.wallet();
    await wallet.signOut();
    setAccountId(null);
    setUsdcBalance(null);
    setStorageBalance(null);
  };

  const status = position?.status ?? (loading ? "Loading" : "Unavailable");
  const collateral = position
    ? formatUnits(position.collateral_wei, 18, 5)
    : "—";
  const debt = position ? formatUnits(position.debt_usdc, 6, 2) : "—";
  const pendingWithdrawal = position?.pending_withdrawal;

  return (
    <div className="shell">
      <header className="topbar">
        <div className="brand">
          <span className="mark" aria-hidden="true">
            NC
          </span>
          <div>
            <strong>Native Credit</strong>
            <span>Sepolia collateral, NEAR credit</span>
          </div>
        </div>
        <div className="top-actions">
          <Badge tone="ok">Testnet only</Badge>
          {accountId ? (
            <div className="account">
              <span>{shortAddress(accountId, 10, 5)}</span>
              <button type="button" className="ghost" onClick={onSignOut}>
                Disconnect
              </button>
            </div>
          ) : (
            <button
              type="button"
              className="primary"
              onClick={() => modal?.show()}
              disabled={!modal}
            >
              Connect NEAR wallet
            </button>
          )}
        </div>
      </header>

      <main>
        <section className="hero">
          <div>
            <span className="eyebrow">NEAR x Ethereum · no bridge</span>
            <h1>
              Your ETH earns credit.
              <br />
              The bridge stays closed.
            </h1>
            <p>
              Sepolia ETH collateral is verified by a light client on NEAR.
              Borrow test USDC against it, repay, and withdraw without wrapping
              or moving the asset through a custodian.
            </p>
          </div>
          <div className="hero-card">
            <div className="hero-card-head">
              <span>Position #{positionId}</span>
              <Badge tone={statusTone(status)}>{status}</Badge>
            </div>
            <label className="id-input">
              Position id
              <input
                type="number"
                min="0"
                value={positionId}
                onChange={(event) =>
                  setPositionId(Math.max(0, Number(event.target.value) || 0))
                }
              />
            </label>
            <div className="hero-grid">
              <Stat
                label="Collateral"
                value={`${collateral} ETH`}
                hint={position?.deposit_address ? "Sepolia EOA" : "Deriving"}
              />
              <Stat
                label="Debt"
                value={`${debt} USDC`}
                hint={`${formatUnits(position?.collateral_wei ?? "0", 18, 2)} ETH locked`}
              />
              <Stat
                label="Health"
                value={healthLabel(health)}
                hint="Liquidation below 1.00x"
                tone={
                  health !== null && BigInt(health) < 10000n ? "danger" : "ok"
                }
              />
              <Stat
                label="Oracle"
                value={priceStale ? "Stale" : "Fresh"}
                hint={
                  priceAge === null
                    ? "No price"
                    : `${priceAge}s old · seq ${price?.sequence ?? "—"}`
                }
                tone={priceStale ? "warn" : "ok"}
              />
            </div>
            {hasPending ? (
              <div className="pending">
                <strong>Proof in flight</strong>
                <span>
                  {pendingWithdrawal
                    ? `Withdrawal ${formatUnits(pendingWithdrawal.amount_wei, 18, 4)} ETH is ${
                        pendingWithdrawal.raw_tx ? "signed" : "awaiting MPC signature"
                      }.`
                    : "A deposit proof is awaiting light-client finality."}
                </span>
              </div>
            ) : null}
          </div>
        </section>

        {error ? (
          <div className="banner error">
            <span>{error}</span>
            <button type="button" className="ghost" onClick={() => setError(null)}>
              Dismiss
            </button>
          </div>
        ) : null}
        {notice ? (
          <div className="banner success">
            <span>{notice}</span>
            <button
              type="button"
              className="ghost"
              onClick={() => setNotice(null)}
            >
              Dismiss
            </button>
          </div>
        ) : null}

        <section className="grid">
          <article className="panel actions">
            <div className="panel-head">
              <div>
                <h2>Manage position</h2>
                <p>
                  {accountId
                    ? isOwner
                      ? "You own this position."
                      : `Owner is ${shortAddress(position?.owner)}.`
                    : "Connect the owner wallet to borrow, repay, or withdraw."}
                </p>
              </div>
              <button type="button" className="ghost" onClick={refresh}>
                Refresh
              </button>
            </div>

            {!accountId ? (
              <>
                <button
                  type="button"
                  className="primary wide"
                  onClick={() => modal?.show()}
                  disabled={!modal}
                >
                  Connect wallet
                </button>
                <div className="action-empty">
                  <div>
                    <span>01</span>
                    <p>Borrow test USDC against verified Sepolia ETH.</p>
                  </div>
                  <div>
                    <span>02</span>
                    <p>Repay through USDC ft_transfer_call and clear debt.</p>
                  </div>
                  <div>
                    <span>03</span>
                    <p>Withdraw ETH only to your registered Sepolia address.</p>
                  </div>
                </div>
              </>
            ) : (
              <>
                {storageBalance === null ? (
                  <div className="inline-note">
                    <span>USDC storage is not registered for this account.</span>
                    <button
                      type="button"
                      className="ghost"
                      onClick={onStorage}
                      disabled={Boolean(busy)}
                    >
                      Register USDC
                    </button>
                  </div>
                ) : null}

                <div className="action-row">
                  <label>
                    Borrow USDC
                    <input
                      inputMode="decimal"
                      value={borrowAmount}
                      onChange={(event) => setBorrowAmount(event.target.value)}
                    />
                  </label>
                  <button
                    type="button"
                    className="primary"
                    onClick={onBorrow}
                    disabled={!canBorrow || Boolean(busy)}
                  >
                    {busy === "Borrow" ? "Borrowing…" : "Borrow"}
                  </button>
                </div>
                {!canBorrow && isOwner ? (
                  <p className="hint">
                    {priceStale
                      ? "Borrow paused: oracle price is stale."
                      : hasPending
                        ? "Borrow paused: another proof is pending."
                        : config?.risk_off
                          ? "Borrow paused: risk-off is active."
                          : "Minimum 1 USDC; borrow stays below the collateral factor."}
                  </p>
                ) : null}

                <div className="action-row">
                  <label>
                    Repay USDC
                    <input
                      inputMode="decimal"
                      value={repayAmount}
                      onChange={(event) => setRepayAmount(event.target.value)}
                    />
                  </label>
                  <button
                    type="button"
                    className="primary"
                    onClick={onRepay}
                    disabled={!canRepay || Boolean(busy)}
                  >
                    {busy === "Repay" ? "Repaying…" : "Repay"}
                  </button>
                </div>
                <p className="hint">
                  Wallet USDC balance:{" "}
                  {usdcBalance === null ? "—" : formatUnits(usdcBalance, 6, 2)}
                </p>

                <div className="action-row">
                  <label>
                    Withdraw ETH
                    <input
                      inputMode="decimal"
                      value={withdrawAmount}
                      onChange={(event) => setWithdrawAmount(event.target.value)}
                    />
                  </label>
                  <button
                    type="button"
                    className="primary"
                    onClick={onWithdraw}
                    disabled={!canWithdraw || Boolean(busy)}
                  >
                    {busy === "Withdraw" ? "Withdrawing…" : "Withdraw"}
                  </button>
                </div>
                {!canWithdraw && isOwner ? (
                  <p className="hint">
                    Withdrawals pause while a proof is pending or the oracle is
                    stale. Minimum 0.01 ETH.
                  </p>
                ) : null}
              </>
            )}
          </article>

          <article className="panel">
            <div className="panel-head">
              <div>
                <h2>Collateral address</h2>
                <p>Deterministic Sepolia EOA derived by NEAR MPC.</p>
              </div>
            </div>
            <div className="address-block">
              <code>{position?.deposit_address ?? "Deriving…"}</code>
              {position?.deposit_address ? (
                <CopyButton value={position.deposit_address} />
              ) : null}
            </div>
            {position?.deposit_address ? (
              <div className="link-row">
                <Link
                  href={`https://sepolia.etherscan.io/address/${position.deposit_address}`}
                >
                  View on Sepolia Etherscan
                </Link>
                <span>
                  Last verified block{" "}
                  {position.last_verified_block?.toLocaleString?.() ??
                    position.last_verified_block}
                </span>
              </div>
            ) : null}

            <div className="divider" />
            <h3>Register a payout address</h3>
            <p className="hint">
              Withdrawals go only to the Sepolia address registered by the
              position owner.
            </p>
            <div className="action-row compact">
              <input
                placeholder="0x…"
                value={sepoliaAddress}
                onChange={(event) => setSepoliaAddress(event.target.value)}
              />
              <button
                type="button"
                className="ghost"
                onClick={onRegisterAddress}
                disabled={!isOwner || !sepoliaAddress || Boolean(busy)}
              >
                Save
              </button>
            </div>
          </article>

          <article className="panel">
            <div className="panel-head">
              <div>
                <h2>Market</h2>
                <p>Team-operated 2-of-3 oracle, refreshed on-chain.</p>
              </div>
              <Badge tone={priceStale ? "warn" : "ok"}>
                {priceStale ? "Stale" : "Fresh"}
              </Badge>
            </div>
            <div className="market">
              <div>
                <span>ETH / USD</span>
                <strong>
                  $
                  {price
                    ? Number(
                        formatUnits(price.eth_usd_1e8, 8, 2).replaceAll(",", ""),
                      ).toLocaleString(undefined, {
                        maximumFractionDigits: 2,
                      })
                    : "—"}
                </strong>
              </div>
              <div>
                <span>USDC / USD</span>
                <strong>
                  $
                  {price
                    ? Number(
                        formatUnits(price.usdc_usd_1e8, 8, 4).replaceAll(",", ""),
                      ).toFixed(4)
                    : "—"}
                </strong>
              </div>
              <div>
                <span>Sequence</span>
                <strong>{price?.sequence ?? "—"}</strong>
              </div>
              <div>
                <span>Age</span>
                <strong>{priceAge === null ? "—" : `${priceAge}s`}</strong>
              </div>
            </div>
            <div className="divider" />
            <h3>Risk parameters</h3>
            <dl className="params">
              <div>
                <dt>Collateral factor</dt>
                <dd>{config ? `${config.collateral_factor_bps / 100}%` : "—"}</dd>
              </div>
              <div>
                <dt>Liquidation threshold</dt>
                <dd>
                  {config ? `${config.liquidation_threshold_bps / 100}%` : "—"}
                </dd>
              </div>
              <div>
                <dt>Liquidation penalty</dt>
                <dd>{config ? `${config.liquidation_penalty_bps / 100}%` : "—"}</dd>
              </div>
              <div>
                <dt>Interest rate</dt>
                <dd>{config ? `${config.interest_rate_bps / 100}%` : "—"}</dd>
              </div>
              <div>
                <dt>Oracle freshness</dt>
                <dd>{config ? `${config.max_oracle_age_secs}s` : "—"}</dd>
              </div>
              <div>
                <dt>Risk-off</dt>
                <dd>{config ? (config.risk_off ? "Active" : "Off") : "—"}</dd>
              </div>
            </dl>
          </article>

          <article className="panel evidence">
            <div className="panel-head">
              <div>
                <h2>Live evidence</h2>
                <p>Every claim links to the chain that produced it.</p>
              </div>
            </div>
            <ul className="evidence-list">
              <li>
                <Badge tone="ok">Real</Badge>
                <div>
                  <strong>Sepolia light-client deposit</strong>
                  <span>
                    finalized block{" "}
                    {position?.last_verified_block ?? "—"} ·{" "}
                    <Link
                      href={`https://testnet.nearblocks.io/txns/${DEPOSIT_TX}`}
                    >
                      record_deposit
                    </Link>
                  </span>
                </div>
              </li>
              <li>
                <Badge tone="ok">Real</Badge>
                <div>
                  <strong>Test USDC repayment</strong>
                  <span>
                    ft_transfer_call ·{" "}
                    <Link
                      href={`https://testnet.nearblocks.io/txns/${REPAY_TX}`}
                    >
                      {shortAddress(REPAY_TX, 10, 6)}
                    </Link>
                  </span>
                </div>
              </li>
              <li>
                <Badge tone="warn">Pending</Badge>
                <div>
                  <strong>MPC-signed ETH withdrawal</strong>
                  <span>
                    broadcast to Sepolia ·{" "}
                    <Link
                      href={`https://sepolia.etherscan.io/tx/${WITHDRAW_TX}`}
                    >
                      {shortAddress(WITHDRAW_TX, 10, 6)}
                    </Link>
                  </span>
                </div>
              </li>
              <li>
                <Badge tone="warn">Simulated</Badge>
                <div>
                  <strong>Test USDC and team oracle</strong>
                  <span>
                    NEAR testnet assets and a 2-of-3 Ed25519 price quorum, not
                    production money.
                  </span>
                </div>
              </li>
            </ul>
            <div className="divider" />
            <div className="contract-links">
              <span>Lending contract</span>
              <Link
                href={`https://testnet.nearblocks.io/address/${CONTRACT_ID}`}
              >
                {CONTRACT_ID}
              </Link>
              <span>USDC</span>
              <Link href={`https://testnet.nearblocks.io/address/${USDC_ID}`}>
                {shortAddress(USDC_ID, 12, 6)}
              </Link>
              <span>RPC</span>
              <code>{NEAR_RPC.replace("https://", "")}</code>
            </div>
          </article>
        </section>

        <section className="trust">
          <div>
            <span className="eyebrow">What is real</span>
            <h2>Verification, not attestation.</h2>
            <p>
              The NEAR contract parses the exact Sepolia header RLP, checks the
              state root, verifies an EIP-1186 EOA proof, and waits for the
              deployed light client to return the matching block hash. A keeper
              cannot invent collateral.
            </p>
          </div>
          <div className="trust-grid">
            <div>
              <strong>No bridge</strong>
              <span>
                ETH never leaves its Sepolia address unless the contract builds
                and MPC signs the transaction.
              </span>
            </div>
            <div>
              <strong>No admin sweep</strong>
              <span>
                Admins can pause new risk, not move collateral. Repay and
                liquidation stay available.
              </span>
            </div>
            <div>
              <strong>Honest limits</strong>
              <span>
                Test USDC, a team oracle, and testnet MPC. The demo labels each
                trust assumption.
              </span>
            </div>
          </div>
        </section>
      </main>

      <footer>
        <span>Native Credit · testnet prototype</span>
        <span>
          <Link href="https://near.org">Built on NEAR</Link> · Sepolia chain id{" "}
          {config?.chain_id ?? "11155111"}
        </span>
      </footer>
    </div>
  );
}
