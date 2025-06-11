use crate::*;

mod extend_post_action_msg_templates {
    use crate::*;

    #[test]
    #[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
    fn test_need_one_yocto() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env.context.predecessor_account_id(owner_id()).build());
        unit_env
            .contract
            .extend_post_action_msg_templates(burrowland_id(), HashSet::new());
    }

    #[test]
    #[should_panic(
        expected = "called `Result::unwrap()` on an `Err` value: HostError(GuestPanic { panic_msg: \"Insufficient permissions for method extend_post_action_msg_templates restricted by access control. Requires one of these roles: [\\\"DAO\\\"]\" })"
    )]
    fn test_need_dao_role() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(user_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env
            .contract
            .extend_post_action_msg_templates(burrowland_id(), HashSet::new());
    }

    #[test]
    #[should_panic(expected = "empty templates.")]
    fn test_empty_templates() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env
            .contract
            .extend_post_action_msg_templates(burrowland_id(), HashSet::new());
    }

    #[test]
    #[should_panic(expected = "\"\" is exist.")]
    fn test_templates_is_exist() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env
            .contract
            .extend_post_action_msg_templates(burrowland_id(), HashSet::from(["".to_string()]));
        unit_env
            .contract
            .extend_post_action_msg_templates(burrowland_id(), HashSet::from(["".to_string()]));
    }

    #[test]
    fn test_succeeded() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env.contract.extend_post_action_msg_templates(
            burrowland_id(),
            HashSet::from(["".to_string(), "".to_string(), "aa".to_string()]),
        );
        let post_action_msg_templates = unit_env
            .contract
            .get_post_action_msg_templates_paged(None, None);
        assert!(post_action_msg_templates.len() == 1);
        let burrowland_msg_templates = post_action_msg_templates.get(&burrowland_id()).unwrap();
        assert!(burrowland_msg_templates.len() == 2);
        assert!(burrowland_msg_templates.contains(""));
        assert!(burrowland_msg_templates.contains("aa"));
    }
}

mod remove_post_action_msg_templates {
    use crate::*;

    #[test]
    #[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
    fn test_need_one_yocto() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env.context.predecessor_account_id(owner_id()).build());
        unit_env
            .contract
            .remove_post_action_msg_templates(burrowland_id(), None);
    }

    #[test]
    #[should_panic(
        expected = "called `Result::unwrap()` on an `Err` value: HostError(GuestPanic { panic_msg: \"Insufficient permissions for method remove_post_action_msg_templates restricted by access control. Requires one of these roles: [\\\"DAO\\\"]\" })"
    )]
    fn test_need_dao_role() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(user_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env
            .contract
            .remove_post_action_msg_templates(burrowland_id(), None);
    }

    #[test]
    #[should_panic(expected = "Invalid contract_id.")]
    fn test_invalid_contract_id1() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env
            .contract
            .remove_post_action_msg_templates(burrowland_id(), None);
    }

    #[test]
    #[should_panic(expected = "Invalid contract_id.")]
    fn test_invalid_contract_id2() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env
            .contract
            .remove_post_action_msg_templates(burrowland_id(), Some(HashSet::new()));
    }

    #[test]
    #[should_panic(expected = "empty templates.")]
    fn test_empty_templates() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env
            .contract
            .extend_post_action_msg_templates(burrowland_id(), HashSet::from(["".to_string()]));
        unit_env
            .contract
            .remove_post_action_msg_templates(burrowland_id(), Some(HashSet::new()));
    }

    #[test]
    #[should_panic(expected = "\"aa\" is not exist.")]
    fn test_templates_is_not_exist() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env
            .contract
            .extend_post_action_msg_templates(burrowland_id(), HashSet::from(["".to_string()]));
        unit_env.contract.remove_post_action_msg_templates(
            burrowland_id(),
            Some(HashSet::from(["aa".to_string()])),
        );
    }

    #[test]
    fn test_succeeded() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());
        unit_env.contract.extend_post_action_msg_templates(
            burrowland_id(),
            HashSet::from([
                "".to_string(),
                "aa".to_string(),
                "bb".to_string(),
                "cc".to_string(),
            ]),
        );
        let post_action_msg_templates = unit_env
            .contract
            .get_post_action_msg_templates_paged(None, None);
        assert!(post_action_msg_templates.len() == 1);
        let burrowland_msg_templates = post_action_msg_templates.get(&burrowland_id()).unwrap();
        assert!(burrowland_msg_templates.len() == 4);
        assert!(burrowland_msg_templates.contains(""));
        assert!(burrowland_msg_templates.contains("aa"));
        assert!(burrowland_msg_templates.contains("bb"));
        assert!(burrowland_msg_templates.contains("cc"));

        unit_env.contract.remove_post_action_msg_templates(
            burrowland_id(),
            Some(HashSet::from(["aa".to_string(), "bb".to_string()])),
        );
        let post_action_msg_templates = unit_env
            .contract
            .get_post_action_msg_templates_paged(None, None);
        assert!(post_action_msg_templates.len() == 1);
        let burrowland_msg_templates = post_action_msg_templates.get(&burrowland_id()).unwrap();
        assert!(burrowland_msg_templates.len() == 2);
        assert!(burrowland_msg_templates.contains(""));
        assert!(!burrowland_msg_templates.contains("aa"));
        assert!(!burrowland_msg_templates.contains("bb"));
        assert!(burrowland_msg_templates.contains("cc"));

        unit_env
            .contract
            .remove_post_action_msg_templates(burrowland_id(), None);

        let post_action_msg_templates = unit_env
            .contract
            .get_post_action_msg_templates_paged(None, None);
        assert!(post_action_msg_templates.is_empty());
    }
}

#[test]
fn test_check_deposit_msg() {
    let mut unit_env = init_unit_env();
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: None,
                extra_msg: None,
            },
            100
        )
        .is_none());

    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![]),
                extra_msg: None,
            },
            100
        )
        .is_none());
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: None
                    },
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: None
                    },
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: None
                    },
                ]),
                extra_msg: None,
            },
            100
        )
        .is_none());
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: None
                    },
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: None
                    },
                ]),
                extra_msg: None,
            },
            100
        )
        .is_none());

    testing_env!(unit_env
        .context
        .predecessor_account_id(owner_id())
        .attached_deposit(NearToken::from_yoctonear(1))
        .build());
    unit_env
        .contract
        .extend_post_action_receiver_id_white_list(vec![burrowland_id()]);
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![PostAction {
                    receiver_id: burrowland_id(),
                    amount: U128(10),
                    memo: None,
                    msg: "".to_string(),
                    gas: Some(Gas::from_tgas(200))
                },]),
                extra_msg: None,
            },
            100
        )
        .is_none());
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: Some(Gas::from_tgas(50))
                    },
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: Some(Gas::from_tgas(10))
                    },
                ]),
                extra_msg: None,
            },
            100
        )
        .is_none());
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: Some(Gas::from_tgas(50))
                    },
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: Some(Gas::from_tgas(100))
                    },
                ]),
                extra_msg: None,
            },
            100
        )
        .is_none());
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "".to_string(),
                        gas: Some(Gas::from_tgas(50))
                    },
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(100),
                        memo: None,
                        msg: "".to_string(),
                        gas: Some(Gas::from_tgas(50))
                    },
                ]),
                extra_msg: None,
            },
            100
        )
        .is_none());
    testing_env!(unit_env
        .context
        .predecessor_account_id(owner_id())
        .attached_deposit(NearToken::from_yoctonear(1))
        .build());
    unit_env
        .contract
        .extend_post_action_msg_templates(burrowland_id(), HashSet::from(["".to_string()]));
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![PostAction {
                    receiver_id: burrowland_id(),
                    amount: U128(10),
                    memo: None,
                    msg: "aa".to_string(),
                    gas: Some(Gas::from_tgas(50))
                },]),
                extra_msg: None,
            },
            100
        )
        .is_none());
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![PostAction {
                    receiver_id: burrowland_id(),
                    amount: U128(10),
                    memo: None,
                    msg: "".to_string(),
                    gas: Some(Gas::from_tgas(50))
                },]),
                extra_msg: None,
            },
            100
        )
        .is_some());
    unit_env.contract.extend_post_action_msg_templates(
        burrowland_id(),
        HashSet::from([r#"{"Execute":{"actions":[{"IncreaseCollateral":{"token_id":"", "amount":"", "max_amount":""}}]}}"#.to_string()]),
    );
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "{\"Execute\":{\"actions\":[{\"IncreaseCollateral\":{\"token_id\":\"17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1\",\"max_amount\":\"1000000000000000000\"}}]}}".to_string(),
                        gas: Some(Gas::from_tgas(50))
                    },
                ]),
                extra_msg: None,
            },
            100
        )
        .is_some());
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "{\"Execute\":{\"actions\":[{\"IncreaseCollateral\":{\"token_id\":\"17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1\",\"amount\":\"1000000000000000000\"}}]}}".to_string(),
                        gas: Some(Gas::from_tgas(50))
                    },
                ]),
                extra_msg: None,
            },
            100
        )
        .is_some());
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "{\"Execute\":{\"actions\":[{\"IncreaseCollateral\":{\"token_id\":\"17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1\"}}]}}".to_string(),
                        gas: Some(Gas::from_tgas(50))
                    },
                ]),
                extra_msg: None,
            },
            100
        )
        .is_some());
    assert!(unit_env
        .contract
        .check_deposit_msg(
            DepositMsg {
                recipient_id: recipient_id(),
                post_actions: Some(vec![
                    PostAction {
                        receiver_id: burrowland_id(),
                        amount: U128(10),
                        memo: None,
                        msg: "{\"Execute\":{\"actions\":[{\"IncreaseCollateral\":{\"token_id\":\"17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1\",\"amount\":\"1000000000000000000\",\"max_amount\":\"1000000000000000000\"}}]}}".to_string(),
                        gas: Some(Gas::from_tgas(50))
                    },
                ]),
                extra_msg: None,
            },
            100
        )
        .is_some());
}

#[test]
fn test_is_structure_equal() {
    // Enum array template
    let template_value: Value = serde_json::from_str(r#"{"Execute":{"actions":[{"IncreaseCollateral":{"token_id":"", "amount":""}}, {"DecreaseCollateral":{"token_id":"", "amount":""}}]}}"#).unwrap();
    let input_value: Value = serde_json::from_str(
        r#"{"Execute":{"actions":[{"IncreaseCollateral":{"token_id":"abc", "amount":"100"}}]}}"#,
    )
    .unwrap();
    assert!(is_structure_equal(&template_value, &input_value));
    let input_value: Value = serde_json::from_str(
        r#"{"Execute":{"actions":[{"DecreaseCollateral":{"token_id":"abc", "amount":"100"}}]}}"#,
    )
    .unwrap();
    assert!(is_structure_equal(&template_value, &input_value));
    let input_value: Value = serde_json::from_str(
        r#"{"Execute":{"actions":[{"DecreaseCollateral":{"token_id":"abc"}}]}}"#,
    )
    .unwrap();
    assert!(is_structure_equal(&template_value, &input_value));

    // wrong key
    let input_value: Value = serde_json::from_str(
        r#"{"Execute":{"actions":[{"DecreaseCollateral":{"token_id":"abc", "amount1":"100"}}]}}"#,
    )
    .unwrap();
    assert!(!is_structure_equal(&template_value, &input_value));
    let input_value: Value = serde_json::from_str(
        r#"{"Execute":{"actions1":[{"DecreaseCollateral":{"token_id":"abc", "amount":"100"}}]}}"#,
    )
    .unwrap();
    assert!(!is_structure_equal(&template_value, &input_value));
    // wrong value
    let input_value: Value = serde_json::from_str(
        r#"{"Execute":{"actions":[{"DecreaseCollateral":{"token_id":"abc", "amount":100}}]}}"#,
    )
    .unwrap();
    assert!(!is_structure_equal(&template_value, &input_value));

    // Regular array template
    let template_value: Value = serde_json::from_str(r#"{"Execute": {"actions":[""]}}"#).unwrap();
    let input_value: Value =
        serde_json::from_str(r#"{"Execute": {"actions":["1", "2"]}}"#).unwrap();
    assert!(is_structure_equal(&template_value, &input_value));

    // null template
    let template_value: Value = serde_json::from_str(r#"{"Execute":{"actions":[null]}}"#).unwrap();
    let input_value: Value =
        serde_json::from_str(r#"{"Execute":{"actions":[{"aa":"bb"}]}}"#).unwrap();
    assert!(is_structure_equal(&template_value, &input_value));
    let input_value: Value = serde_json::from_str(r#"{"Execute":{"actions":[1, 2]}}"#).unwrap();
    assert!(is_structure_equal(&template_value, &input_value));

    let template_value: Value = serde_json::from_str(r#"{"Execute":{"actions":null}}"#).unwrap();
    let input_value: Value = serde_json::from_str(r#"{"Execute":{"actions":[]}}"#).unwrap();
    assert!(is_structure_equal(&template_value, &input_value));
    let input_value: Value = serde_json::from_str(r#"{"Execute":{"actions":1}}"#).unwrap();
    assert!(is_structure_equal(&template_value, &input_value));
    let input_value: Value = serde_json::from_str(r#"{"Execute":{"actions":"aa"}}"#).unwrap();
    assert!(is_structure_equal(&template_value, &input_value));
}
