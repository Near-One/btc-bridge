use crate::*;

const MAX_POST_ACTIONS_NUM: usize = 2;
const MAX_TOTAL_POST_ACTIONS_GAS: Gas = Gas::from_tgas(130);
const MAX_PER_POST_ACTIONS_GAS: Gas = Gas::from_tgas(100);
const MIN_PER_POST_ACTIONS_GAS: Gas = Gas::from_tgas(30);

#[near(serializers = [json])]
#[derive(Clone)]
pub struct DepositMsg {
    // The NEAR account receiving nBTC.
    pub recipient_id: AccountId,
    // Parameters for executing ft_transfer_call after successful nBTC minting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_actions: Option<Vec<PostAction>>,
    // Used to support other dApps extending based on verify_deposit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_msg: Option<String>,
}

#[near(serializers = [json])]
#[derive(Clone)]
pub struct PostAction {
    pub receiver_id: AccountId,
    pub amount: U128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas: Option<Gas>,
}

pub fn get_deposit_path(deposit_msg: &DepositMsg) -> String {
    let deposit_msg_string = serde_json::to_string(&deposit_msg).unwrap();
    hex::encode(env::sha256(deposit_msg_string.as_bytes()))
}

impl Contract {
    pub fn check_deposit_msg(
        &self,
        deposit_msg: DepositMsg,
        actual_mintable_amount: u128,
        utxo_tx_id: String,
    ) -> Option<Vec<PostAction>> {
        let mut post_actions = deposit_msg.post_actions?;
        if post_actions.is_empty() {
            Event::InvalidPostAction {
                index: None,
                err_msg: "empty post_actions.".to_string(),
            }
            .emit();
            return None;
        }
        // post_actions supports at most two.
        if post_actions.len() > MAX_POST_ACTIONS_NUM {
            Event::InvalidPostAction {
                index: None,
                err_msg: format!(
                    "The number({}) of post_actions exceeds the limit of {}.",
                    post_actions.len(),
                    MAX_POST_ACTIONS_NUM
                ),
            }
            .emit();
            return None;
        }
        let mut total_gas = 0;
        let mut total_amount = 0;
        for (index, post_action) in post_actions.iter_mut().enumerate() {
            total_amount += post_action.amount.0;
            // The receiver_id must be on the whitelist.
            if !self
                .data()
                .post_action_receiver_id_white_list
                .contains(&post_action.receiver_id)
            {
                Event::InvalidPostAction {
                    index: Some(index),
                    err_msg: format!(
                        "The receiver_id({}) of the post_action is not on the whitelist.",
                        post_action.receiver_id
                    ),
                }
                .emit();
                return None;
            }
            if let Some(msg_templates) = self
                .data()
                .post_action_msg_templates
                .get(&post_action.receiver_id)
            {
                let updated_post_action: Option<String> =
                    match serde_json::from_str::<Value>(&post_action.msg) {
                        Ok(msg_value) => {
                            let mut res = None;
                            for template in msg_templates {
                                if let Ok(template_value) = serde_json::from_str::<Value>(template) { 
                                    res = check_template_and_update_msg(&template_value, &msg_value, &utxo_tx_id);
                                    if res.is_some() {
                                        break;
                                    }
                                }
                            }
                            
                            res.map(|x| x.to_string())
                        },
                        Err(_) => {
                            if msg_templates
                                .iter()
                                .any(|template| template == &post_action.msg) {
                                Some(post_action.msg.clone())
                        } else {
                                None
                            }
                    },
                    };
                
                if let Some(updated_post_action) = updated_post_action {
                    post_action.msg = updated_post_action;
                } else {
                    Event::InvalidPostAction {
                        index: Some(index),
                        err_msg: "Unsupported post_action.msg.".to_string(),
                    }
                    .emit();
                    return None;
                }
            }
            if let Some(gas) = post_action.gas {
                // The gas specified by a single post_action must be between 30 Tgas and 100 Tgas, inclusive.
                if gas.as_gas() > MAX_PER_POST_ACTIONS_GAS.as_gas() {
                    Event::InvalidPostAction {
                        index: Some(index),
                        err_msg: format!(
                            "The amount({}) of gas exceeds the limit of {}.",
                            gas, MAX_PER_POST_ACTIONS_GAS
                        ),
                    }
                    .emit();
                    return None;
                }
                if gas.as_gas() < MIN_PER_POST_ACTIONS_GAS.as_gas() {
                    Event::InvalidPostAction {
                        index: Some(index),
                        err_msg: format!(
                            "The gas amount({}) does not meet the minimum requirement of {}.",
                            gas, MIN_PER_POST_ACTIONS_GAS
                        ),
                    }
                    .emit();
                    return None;
                }
                total_gas += gas.as_gas();
            }
        }
        // The total gas for all post_actions must not exceed 130 Tgas.
        if total_gas > MAX_TOTAL_POST_ACTIONS_GAS.as_gas() {
            Event::InvalidPostAction {
                index: None,
                err_msg: format!(
                    "The total amount({}) of gas exceeds the limit of {}.",
                    total_gas, MAX_TOTAL_POST_ACTIONS_GAS
                ),
            }
            .emit();
            return None;
        }
        if total_amount > actual_mintable_amount {
            Event::InvalidPostAction {
                index: None,
                err_msg: format!(
                    "The total amount({}) of nBTC used in post_actions exceeds the mint amount ({}).",
                    total_amount, actual_mintable_amount
                ),
            }
            .emit();
            return None;
        }
        Some(post_actions)
    }
}
