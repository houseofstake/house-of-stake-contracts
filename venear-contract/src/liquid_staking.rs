use crate::*;
use common::{Fraction, TimestampNs};

#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct LstData {
    pub timestamp: TimestampNs,
    pub multiplier: Fraction,
}

#[near]
impl Contract {
    pub fn refresh_lst_data(&mut self) {
        self.internal_refresh_stnear();
        self.internal_refresh_linear();
    }
}

impl Contract {
    pub fn internal_refresh_stnear(&mut self) {
        if let Some(stnear_account_id) = self.config.stnear_account_id.as_ref() {
            todo!()
        }
    }

    pub fn internal_refresh_linear(&mut self) {
        if let Some(linear_account_id) = self.config.linear_account_id.as_ref() {
            todo!()
        }
    }

    pub fn internal_update_lst_data(&mut self, token_id: AccountId, lst_data: LstData) {
        let prev_value = self
            .lsts
            .get_mut()
            .as_mut()
            .unwrap()
            .insert(token_id, lst_data.clone());
        if let Some(prev_value) = prev_value {
            require!(
                lst_data.timestamp > prev_value.timestamp,
                "New LST timestamp is not greater"
            );
            require!(lst_data.multiplier >= prev_value.multiplier);
        }
    }
}
