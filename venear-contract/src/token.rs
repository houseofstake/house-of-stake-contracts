use crate::*;

const ICON_BASE64: &str = "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTAwIiBoZWlnaHQ9IjEwMCIgdmlld0JveD0iMCAwIDEwMCAxMDAiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgZmlsbD0ibm9uZSI+CiAgPHJlY3Qgd2lkdGg9IjEwMCIgaGVpZ2h0PSIxMDAiIHJ4PSIyMCIgZmlsbD0iIzAwMCIvPgogIDx0ZXh0IHg9IjQwIiB5PSI2NSIgZm9udC1mYW1pbHk9IkFyaWFsLCBzYW5zLXNlcmlmIiBmb250LXNpemU9IjYwIiBmaWxsPSIjZmZmIiBmb250LXdlaWdodD0iYm9sZCI+TjwvdGV4dD4KICA8dGV4dCB4PSIxNSIgeT0iNjUiIGZvbnQtZmFtaWx5PSJBcmlhbCwgc2Fucy1zZXJpZiIgZm9udC1zaXplPSIyMCIgZmlsbD0iI2ZmZiI+dmU8L3RleHQ+Cjwvc3ZnPgo=";

#[near]
impl Contract {
    /// Returns the balance of the account in the veNEAR.
    pub fn ft_balance_of(&self, account_id: AccountId) -> NearToken {
        self.internal_get_account(&account_id)
            .map(|account| {
                account
                    .venear_balance(
                        env::block_timestamp().into(),
                        self.internal_get_venear_growth_config(),
                    )
                    .total()
            })
            .unwrap_or_default()
    }

    /// Returns the total supply of the veNEAR.
    pub fn ft_total_supply(&self) -> NearToken {
        self.internal_global_state_updated()
            .total_venear_balance
            .total()
    }

    /// Method to match the fungible token interface. Can't be called.
    #[payable]
    pub fn ft_transfer(&mut self) {
        env::panic_str("Non transferable token");
    }

    /// Method to match the fungible token interface. Can't be called.
    #[payable]
    pub fn ft_transfer_call(&mut self) {
        env::panic_str("Non transferable token");
    }

    /// Returns the metadata of the veNEAR fungible token.
    pub fn ft_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "spec": "ft-1.0.0",
            "name": "veNEAR",
            "symbol": "VENEAR",
            "icon": ICON_BASE64,
            "reference": null,
            "reference_hash": null,
            "decimals": 24
        })
    }
}
