pub const BPS: u128 = 10_000;
pub const YEAR_SECS: u128 = 365 * 24 * 60 * 60;
pub const ETH_DECIMALS: u8 = 18;
pub const USDC_DECIMALS: u8 = 6;
pub const USD_DECIMALS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathError {
    Overflow,
    DivisionByZero,
}

pub fn usd_value(amount: u128, price_1e8: u128, decimals: u8) -> Result<u128, MathError> {
    let scale = 10u128
        .checked_pow(decimals as u32)
        .ok_or(MathError::Overflow)?;
    amount
        .checked_mul(price_1e8)
        .ok_or(MathError::Overflow)?
        .checked_div(scale)
        .ok_or(MathError::DivisionByZero)
}

pub fn health_factor_bps(
    collateral_usd: u128,
    debt_usd: u128,
    liquidation_threshold_bps: u16,
) -> Result<u128, MathError> {
    if debt_usd == 0 {
        return Ok(u128::MAX);
    }
    collateral_usd
        .checked_mul(liquidation_threshold_bps as u128)
        .ok_or(MathError::Overflow)?
        .checked_div(debt_usd)
        .ok_or(MathError::DivisionByZero)
}

pub fn max_borrow_usd(collateral_usd: u128, collateral_factor_bps: u16) -> Result<u128, MathError> {
    collateral_usd
        .checked_mul(collateral_factor_bps as u128)
        .ok_or(MathError::Overflow)?
        .checked_div(BPS)
        .ok_or(MathError::DivisionByZero)
}

pub fn liquidation_collateral_wei(
    repay_usd: u128,
    penalty_bps: u16,
    eth_price_1e8: u128,
    collateral_wei: u128,
) -> Result<u128, MathError> {
    let seized_usd = repay_usd
        .checked_mul(BPS + penalty_bps as u128)
        .ok_or(MathError::Overflow)?
        .checked_div(BPS)
        .ok_or(MathError::DivisionByZero)?;
    let seized_wei = seized_usd
        .checked_mul(10u128.pow(ETH_DECIMALS as u32))
        .ok_or(MathError::Overflow)?
        .checked_div(eth_price_1e8)
        .ok_or(MathError::DivisionByZero)?;
    Ok(seized_wei.min(collateral_wei))
}

pub fn max_liquidation_repay_usd(
    collateral_usd: u128,
    penalty_bps: u16,
) -> Result<u128, MathError> {
    collateral_usd
        .checked_mul(BPS)
        .ok_or(MathError::Overflow)?
        .checked_div(
            BPS.checked_add(penalty_bps as u128)
                .ok_or(MathError::Overflow)?,
        )
        .ok_or(MathError::DivisionByZero)
}

pub fn token_value(usd_1e8: u128, price_1e8: u128, decimals: u8) -> Result<u128, MathError> {
    let scale = 10u128
        .checked_pow(decimals as u32)
        .ok_or(MathError::Overflow)?;
    usd_1e8
        .checked_mul(scale)
        .ok_or(MathError::Overflow)?
        .checked_div(price_1e8)
        .ok_or(MathError::DivisionByZero)
}

pub fn accrue_interest(
    debt: u128,
    annual_rate_bps: u16,
    elapsed_secs: u64,
) -> Result<u128, MathError> {
    if debt == 0 || annual_rate_bps == 0 || elapsed_secs == 0 {
        return Ok(debt);
    }
    let interest = debt
        .checked_mul(annual_rate_bps as u128)
        .ok_or(MathError::Overflow)?
        .checked_mul(elapsed_secs as u128)
        .ok_or(MathError::Overflow)?
        .checked_div(BPS.checked_mul(YEAR_SECS).ok_or(MathError::Overflow)?)
        .ok_or(MathError::DivisionByZero)?;
    debt.checked_add(interest).ok_or(MathError::Overflow)
}

pub fn available_collateral(collateral: u128, pending_outflow: u128) -> Result<u128, MathError> {
    collateral
        .checked_sub(pending_outflow)
        .ok_or(MathError::Overflow)
}

pub fn is_liquidatable(health_bps: u128) -> bool {
    health_bps < BPS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usd_value_eth_and_usdc() {
        assert_eq!(
            usd_value(10u128.pow(18), 300_000_000_000, 18).unwrap(),
            300_000_000_000
        );
        assert_eq!(usd_value(1_000_000, 100_000_000, 6).unwrap(), 100_000_000);
    }

    #[test]
    fn health_factor_zero_debt_is_infinite() {
        assert_eq!(health_factor_bps(100, 0, 8000).unwrap(), u128::MAX);
    }

    #[test]
    fn health_factor_and_liquidation_boundary() {
        assert_eq!(health_factor_bps(1000, 1000, 8000).unwrap(), 8000);
        assert!(is_liquidatable(9999));
        assert!(!is_liquidatable(10000));
    }

    #[test]
    fn borrow_limit() {
        assert_eq!(max_borrow_usd(1000, 7500).unwrap(), 750);
    }

    #[test]
    fn liquidation_seizes_penalty_and_caps() {
        let seized =
            liquidation_collateral_wei(100_000_000, 800, 300_000_000_000, 10u128.pow(18)).unwrap();
        assert_eq!(seized, 360_000_000_000_000);
        let capped = liquidation_collateral_wei(100_000_000, 800, 300_000_000_000, 1).unwrap();
        assert_eq!(capped, 1);
    }

    #[test]
    fn max_liquidation_repay_leaves_room_for_penalty() {
        assert_eq!(
            max_liquidation_repay_usd(100_000_000, 800).unwrap(),
            92_592_592
        );
        assert_eq!(
            max_liquidation_repay_usd(108_000_000, 800).unwrap(),
            100_000_000
        );
    }

    #[test]
    fn token_value_converts_usd_back_to_base_units() {
        assert_eq!(token_value(100_000_000, 100_000_000, 6).unwrap(), 1_000_000);
        assert_eq!(
            token_value(300_000_000_000, 300_000_000_000, 18).unwrap(),
            10u128.pow(18)
        );
        assert_eq!(token_value(1, 100_000_000, 6).unwrap(), 0);
        assert_eq!(token_value(1, 0, 6), Err(MathError::DivisionByZero));
    }

    #[test]
    fn interest_accrual() {
        let one_year = 31_536_000u64;
        assert_eq!(
            accrue_interest(1_000_000, 500, one_year).unwrap(),
            1_050_000
        );
        assert_eq!(accrue_interest(1_000_000, 500, 0).unwrap(), 1_000_000);
    }

    #[test]
    fn overflow_is_rejected() {
        assert_eq!(usd_value(u128::MAX, 2, 18), Err(MathError::Overflow));
        assert_eq!(
            accrue_interest(u128::MAX, 10_000, 31_536_000),
            Err(MathError::Overflow)
        );
    }

    #[test]
    fn pending_outflow_cannot_exceed_collateral() {
        assert_eq!(available_collateral(10, 4).unwrap(), 6);
        assert_eq!(available_collateral(4, 10), Err(MathError::Overflow));
    }
}
