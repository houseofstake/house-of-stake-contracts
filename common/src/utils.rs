use crate::*;

pub fn near_add(a: NearToken, b: NearToken) -> NearToken {
    a.checked_add(b).unwrap()
}

pub fn near_sub(a: NearToken, b: NearToken) -> NearToken {
    a.checked_sub(b).unwrap()
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
