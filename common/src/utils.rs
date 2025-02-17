use crate::*;

pub fn near_add(a: NearToken, b: NearToken) -> NearToken {
    NearToken::from_yoctonear(a.as_yoctonear() + b.as_yoctonear())
}

pub fn near_sub(a: NearToken, b: NearToken) -> NearToken {
    NearToken::from_yoctonear(a.as_yoctonear() - b.as_yoctonear())
}
