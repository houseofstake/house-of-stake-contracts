use near_sdk::serde::Serialize;

pub mod emit {
    use super::*;
    use crate::TimestampNs;
    use near_sdk::json_types::U64;
    use near_sdk::{log, AccountId, NearToken};

    #[derive(Serialize)]
    #[serde(crate = "near_sdk::serde")]
    pub(crate) struct LockupUpdateData<'a> {
        pub(crate) account_id: &'a AccountId,
        pub(crate) lockup_version: u64,
        pub(crate) timestamp: &'a Option<TimestampNs>,
        pub(crate) lockup_update_nonce: &'a Option<U64>,
        pub(crate) locked_near_balance: &'a Option<NearToken>,
    }

    #[derive(Serialize)]
    #[serde(crate = "near_sdk::serde")]
    pub(crate) struct ProposalVoteData<'a> {
        pub(crate) account_id: &'a AccountId,
        pub(crate) proposal_id: u32,
        pub(crate) vote: u32,
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

    #[derive(Serialize)]
    #[serde(crate = "near_sdk::serde")]
    pub(crate) struct EventJson<'a, T>
    where
        T: Serialize,
    {
        pub(crate) standard: &'a str,
        pub(crate) version: &'a str,
        pub(crate) event: &'a str,
        pub(crate) data: &'a [T],
    }

    fn log_event<T: Serialize>(event: &str, data: T) {
        log!(
            "EVENT_JSON:{}",
            serde_json::to_string(&EventJson {
                standard: "venear",
                version: "0.1.0",
                event,
                data: &[data],
            })
            .unwrap()
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::json_types::U64;
    use near_sdk::serde::Serialize;
    use near_sdk::NearToken;
    use near_sdk::{serde_json, AccountId};

    #[test]
    fn test_option_u64_serializer() {
        #[derive(Serialize)]
        struct TestStruct {
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
        assert_eq!(json, r#"{"value":null}"#);
    }

    #[test]
    fn test_option_near_token_serializer() {
        #[derive(Serialize)]
        struct TestStruct {
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
        assert_eq!(json, r#"{"value":null}"#);
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
            r#"{"account_id":"test.near","lockup_version":1,"timestamp":null,"lockup_update_nonce":null,"locked_near_balance":null}"#
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
        let _expected_log = format!(
            r#"EVENT_JSON:{{"standard":"venear","version":"0.1.0","event":"test_event","data":[{{"account_id":"event_test.near","lockup_version":1,"timestamp":"987654321","lockup_update_nonce":"777","locked_near_balance":"5555555555555555555"}}]}}"#
        );
        // Normally you would check the actual logs here
    }
}
