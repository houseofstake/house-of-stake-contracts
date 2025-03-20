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
        pub(crate) lockup_version: u64,
        #[serde(with = "option_u64_dec_format")]
        pub(crate) timestamp: &'a Option<TimestampNs>,
        #[serde(with = "option_u64_dec_format")]
        pub(crate) lockup_update_nonce: &'a Option<U64>,
        #[serde(with = "option_u128_dec_format")]
        pub(crate) locked_near_balance: &'a Option<NearToken>,
    }

    #[derive(Serialize)]
    #[serde(crate = "near_sdk::serde")]
    pub(crate) struct ProposalVoteData<'a> {
        pub(crate) account_id: &'a AccountId,
        pub(crate) proposal_id: u32,
        pub(crate) vote: u32,
        #[serde(with = "u128_dec_format")]
        pub(crate) account_balance: &'a NearToken,
    }

    #[derive(Serialize)]
    #[serde(crate = "near_sdk::serde")]
    pub(crate) struct VotingProposalUpdateData<'a> {
        pub(crate) account_id: &'a AccountId,
        pub(crate) proposal_id: u32,
        pub(crate) voting_start_time_sec: Option<u32>,
    }

    #[derive(Serialize)]
    #[serde(crate = "near_sdk::serde")]
    pub(crate) struct ProposalData<'a> {
        pub(crate) proposer_id: &'a AccountId,
        pub(crate) proposal_id: u32,
        pub(crate) title: &'a Option<String>,
        pub(crate) description: &'a Option<String>,
        pub(crate) link: &'a Option<String>,
        pub(crate) voting_options: &'a Vec<String>,
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
        lockup_version: u64,
        lockup_update_nonce: &Option<U64>,
        timestamp: &Option<TimestampNs>,
        locked_near_balance: &Option<NearToken>,
    ) {
        log_event(
            action,
            LockupUpdateData {
                account_id,
                lockup_version,
                lockup_update_nonce,
                timestamp,
                locked_near_balance,
            },
        );
    }

    pub fn proposal_vote_action(
        action: &str,
        account_id: &AccountId,
        proposal_id: u32,
        vote: u32,
        account_balance: &NearToken,
    ) {
        log_event(
            action,
            ProposalVoteData {
                account_id,
                proposal_id,
                vote,
                account_balance,
            },
        );
    }

    pub fn approve_proposal_action(
        action: &str,
        account_id: &AccountId,
        proposal_id: u32,
        voting_start_time_sec: Option<u32>,
    ) {
        log_event(
            action,
            VotingProposalUpdateData {
                account_id,
                proposal_id,
                voting_start_time_sec,
            },
        );
    }

    pub fn create_proposal_action(
        action: &str,
        proposer_id: &AccountId,
        proposal_id: u32,
        title: &Option<String>,
        description: &Option<String>,
        link: &Option<String>,
        voting_options: &Vec<String>,
    ) {
        log_event(
            action,
            ProposalData {
                proposer_id,
                proposal_id,
                title,
                description,
                link,
                voting_options,
            },
        );
    }
}

pub mod u128_dec_format {
    use near_sdk::serde::Serializer;
    use near_sdk::NearToken;

    pub fn serialize<S>(num: &NearToken, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&num.as_yoctonear().to_string())
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
        let version: u64 = 1;

        let test_data = emit::LockupUpdateData {
            account_id: &account_id,
            lockup_version: version,
            timestamp: &timestamp,
            lockup_update_nonce: &nonce,
            locked_near_balance: &balance,
        };

        let json = serde_json::to_string(&test_data).unwrap();
        assert_eq!(
            json,
            r#"{"account_id":"test.near","lockup_version":1,"timestamp":"123456789","lockup_update_nonce":"42","locked_near_balance":"1000000000000000000000000"}"#
        );

        // Test with None values
        let test_data = emit::LockupUpdateData {
            account_id: &account_id,
            lockup_version: version,
            timestamp: &None,
            lockup_update_nonce: &None,
            locked_near_balance: &None,
        };

        let json = serde_json::to_string(&test_data).unwrap();
        assert_eq!(
            json,
            r#"{"account_id":"test.near","lockup_version":1,"timestamp":"0","lockup_update_nonce":"0","locked_near_balance":"0"}"#
        );
    }

    #[test]
    fn test_event_log_format() {
        let account_id: AccountId = "event_test.near".parse().unwrap();
        let nonce = Some(U64(777));
        let timestamp = Some(U64(987654321987654321));
        let balance = Some(NearToken::from_yoctonear(5555555555555555555));
        let version: u64 = 1;

        emit::lockup_action(
            "test_event",
            &account_id,
            version,
            &nonce,
            &timestamp,
            &balance,
        );

        // The actual log would need to be captured and verified
        // This is just a format check example
        let expected_log = format!(
            r#"EVENT_JSON:{{"standard":"venear","version":"0.1.0","event":"test_event","data":[{{"account_id":"event_test.near","lockup_version":1,"timestamp":"987654321","lockup_update_nonce":"777","locked_near_balance":"5555555555555555555"}}]}}"#
        );
        // Normally you would check the actual logs here
    }
}
