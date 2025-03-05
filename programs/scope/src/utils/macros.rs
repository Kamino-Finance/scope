#[macro_export]
macro_rules! assert_fuzzy_eq {
    ($actual:expr, $expected:expr, $epsilon:expr) => {
        let eps = $epsilon as i128;
        let act = $actual as i128;
        let exp = $expected as i128;
        let diff = (act - exp).abs();
        if diff > eps {
            panic!(
                "Actual {} Expected {} diff {} Epsilon {}",
                $actual, $expected, diff, eps
            );
        }
    };

    ($actual:expr, $expected:expr, $epsilon:expr, $type:ty) => {
        let eps = $epsilon as $type;
        let act = $actual as $type;
        let exp = $expected as $type;
        let diff = (act - exp).abs();
        if diff > eps {
            panic!(
                "Actual {} Expected {} diff {} Epsilon {}",
                $actual, $expected, diff, eps
            );
        }
    };
}

#[macro_export]
macro_rules! assert_fuzzy_price_eq {
    ($actual:expr, $expected:expr, $epsilon:expr, $($t:tt)*) => {
        let eps: ::decimal_wad::decimal::Decimal = $epsilon.into();
        let act: ::decimal_wad::decimal::Decimal = $actual.into();
        let exp: ::decimal_wad::decimal::Decimal = $expected.into();
        let diff = if act > exp { act - exp } else { exp - act };
        if diff > eps {
            let msg = format!($($t)*);
            panic!(
                "{} Actual {} Expected {} diff {} Epsilon {}",
                msg, act, exp, diff, eps
            );
        }
    };
}

#[cfg(any(target_os = "solana", not(feature = "tracing")))]
#[macro_export]
macro_rules! debug {
    ($($t:tt)*) => {
        solana_program::msg!($($t)*);
    };
}

#[cfg(any(target_os = "solana", not(feature = "tracing")))]
#[macro_export]
macro_rules! info {
    ($($t:tt)*) => {
        solana_program::msg!($($t)*);
    };
}

#[cfg(any(target_os = "solana", not(feature = "tracing")))]
#[macro_export]
macro_rules! warn {
    ($($t:tt)*) => {
        solana_program::msg!($($t)*);
    };
}

#[cfg(all(not(target_os = "solana"), feature = "tracing"))]
#[macro_export]
macro_rules! debug {
    ($($t:tt)*) => {
        tracing::debug!($($t)*);
    };
}

#[cfg(all(not(target_os = "solana"), feature = "tracing"))]
#[macro_export]
macro_rules! info {
    ($($t:tt)*) => {
        tracing::info!($($t)*);
    };
}

#[cfg(all(not(target_os = "solana"), feature = "tracing"))]
#[macro_export]
macro_rules! warn {
    ($($t:tt)*) => {
        tracing::warn!($($t)*);
    };
}
