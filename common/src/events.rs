use near_sdk::serde::Serialize;

pub mod emit {
    use super::*;
    use crate::TimestampNs;
    use near_sdk::json_types::U64;
    use near_sdk::serde_json::json;
    use near_sdk::{log, AccountId, NearToken};

    #[derive(Serialize)]
    #[serde(crate = "near_sdk::serde")]
    pub(crate) struct LockupUpdateData<'a> {
        pub(crate) account_id: &'a AccountId,
        #[serde(with = "option_u64_dec_format")]
        pub(crate) timestamp: &'a Option<TimestampNs>,
        #[serde(with = "option_u64_dec_format")]
        pub(crate) lockup_update_nonce: &'a Option<U64>,
        #[serde(with = "option_u128_dec_format")]
        pub(crate) locked_near_balance: &'a Option<NearToken>,
    }

    fn log_event<T: Serialize>(event: &str, data: T) {
        let event = json!({
            "standard": "venear",
            "version": "0.1.0",
            "event": event,
            "data": [data]
        });

        log!("EVENT_JSON:{}", event.to_string());
    }

    pub fn lockup_action(
        action: &str,
        account_id: &AccountId,
        lockup_update_nonce: &Option<U64>,
        timestamp: &Option<TimestampNs>,
        locked_near_balance: &Option<NearToken>,
    ) {
        log_event(
            action,
            LockupUpdateData {
                account_id,
                lockup_update_nonce,
                timestamp,
                locked_near_balance,
            },
        );
    }
}

pub mod option_u128_dec_format {
    use near_sdk::serde::Serializer;
    use near_sdk::NearToken;

    pub fn serialize<S>(num: &Option<NearToken>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(
            &num.as_ref()
                .map_or_else(|| "0".to_string(), |n| n.as_yoctonear().to_string()),
        )
    }
}

pub mod option_u64_dec_format {
    use near_sdk::json_types::U64;
    use near_sdk::serde::Serializer;

    pub fn serialize<S>(value: &Option<U64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(
            &value
                .as_ref()
                .map_or_else(|| "0".to_string(), |v| v.0.to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::json_types::U64;
    use near_sdk::serde::Serialize;
    use near_sdk::NearToken;
    use near_sdk::{serde_json, AccountId};

    type TimestampNs = U64; // Assuming original type alias

    #[test]
    fn test_option_u64_serializer() {
        #[derive(Serialize)]
        struct TestStruct {
            #[serde(with = "option_u64_dec_format")]
            value: Option<U64>,
        }

        // Test Some value
        let test = TestStruct {
            value: Some(U64(123456789)),
        };
        let json = serde_json::to_string(&test).unwrap();
        assert_eq!(json, r#"{"value":"123456789"}"#);

        // Test None value
        let test = TestStruct { value: None };
        let json = serde_json::to_string(&test).unwrap();
        assert_eq!(json, r#"{"value":"0"}"#);
    }

    #[test]
    fn test_option_near_token_serializer() {
        #[derive(Serialize)]
        struct TestStruct {
            #[serde(with = "option_u128_dec_format")]
            value: Option<NearToken>,
        }

        // Test Some value
        let test = TestStruct {
            value: Some(NearToken::from_yoctonear(987654321)),
        };
        let json = serde_json::to_string(&test).unwrap();
        assert_eq!(json, r#"{"value":"987654321"}"#);

        // Test None value
        let test = TestStruct { value: None };
        let json = serde_json::to_string(&test).unwrap();
        assert_eq!(json, r#"{"value":"0"}"#);
    }

    #[test]
    fn test_full_struct_serialization() {
        let account_id: AccountId = "test.near".parse().unwrap();
        let nonce = Some(U64(42));
        let timestamp = Some(U64(123456789)); // Using U64 for TimestampNs
        let balance = Some(NearToken::from_yoctonear(1000000000000000000000000));

        let test_data = emit::LockupUpdateData {
            account_id: &account_id,
            timestamp: &timestamp,
            lockup_update_nonce: &nonce,
            locked_near_balance: &balance,
        };

        let json = serde_json::to_string(&test_data).unwrap();
        assert_eq!(
            json,
            r#"{"account_id":"test.near","timestamp":"123456789","lockup_update_nonce":"42","locked_near_balance":"1000000000000000000000000"}"#
        );

        // Test with None values
        let test_data = emit::LockupUpdateData {
            account_id: &account_id,
            timestamp: &None,
            lockup_update_nonce: &None,
            locked_near_balance: &None,
        };

        let json = serde_json::to_string(&test_data).unwrap();
        assert_eq!(
            json,
            r#"{"account_id":"test.near","timestamp":"0","lockup_update_nonce":"0","locked_near_balance":"0"}"#
        );
    }

    #[test]
    fn test_event_log_format() {
        let account_id: AccountId = "event_test.near".parse().unwrap();
        let nonce = Some(U64(777));
        let timestamp = Some(U64(987654321987654321));
        let balance = Some(NearToken::from_yoctonear(5555555555555555555));

        emit::lockup_action("test_event", &account_id, &nonce, &timestamp, &balance);

        // The actual log would need to be captured and verified
        // This is just a format check example
        let expected_log = format!(
            r#"EVENT_JSON:{{"standard":"venear","version":"0.1.0","event":"test_event","data":[{{"account_id":"event_test.near","timestamp":"987654321","lockup_update_nonce":"777","locked_near_balance":"5555555555555555555"}}]}}"#
        );
        // Normally you would check the actual logs here
    }
}
