use crate::*;

pub fn near_add(a: NearToken, b: NearToken) -> NearToken {
    a.checked_add(b).unwrap()
}

pub fn near_sub(a: NearToken, b: NearToken) -> NearToken {
    a.checked_sub(b).unwrap()
}

pub fn truncate_to_seconds(nanoseconds: TimestampNs) -> TimestampNs {
    (nanoseconds.0 / 1_000_000_000 * 1_000_000_000).into()
}

pub fn truncate_near_to_millis(near: NearToken) -> NearToken {
    NearToken::from_millinear(near.as_millinear())
}

// Tests for `near_add` and `near_sub`
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_near_add() {
        let a = NearToken::from_yoctonear(100);
        let b = NearToken::from_yoctonear(200);
        let c = NearToken::from_yoctonear(300);
        assert_eq!(near_add(a, b), c);
    }

    #[test]
    fn test_near_sub() {
        let a = NearToken::from_yoctonear(300);
        let b = NearToken::from_yoctonear(200);
        let c = NearToken::from_yoctonear(100);
        assert_eq!(near_sub(a, b), c);
    }
}
