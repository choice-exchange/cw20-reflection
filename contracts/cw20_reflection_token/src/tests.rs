#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query, TREASURY};
    use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
    use choice::mock_querier::{mock_dependencies, WasmMockQuerier};
    use cosmwasm_std::testing::{message_info, mock_env, MockApi};
    use cosmwasm_std::{from_json, to_json_binary, Addr, CosmosMsg, Decimal, Uint128, WasmMsg};
    use cw20::{BalanceResponse, Cw20ReceiveMsg};
    use serde::Serialize;

    struct TestAddresses {
        admin: Addr,
        user_a: Addr,
        user_b: Addr,
        aggregator: Addr,
        pair: Addr,
        treasury: Addr,
    }

    fn setup_test() -> (
        cosmwasm_std::OwnedDeps<cosmwasm_std::MemoryStorage, MockApi, WasmMockQuerier>,
        cosmwasm_std::Env,
        TestAddresses,
    ) {
        let mut deps = mock_dependencies(&[]);
        let env = mock_env();

        let addrs = TestAddresses {
            admin: deps.api.addr_make("admin"),
            user_a: deps.api.addr_make("user_a"),
            user_b: deps.api.addr_make("user_b"),
            aggregator: deps.api.addr_make("aggregator"),
            pair: deps.api.addr_make("pair"),
            treasury: deps.api.addr_make("treasury"),
        };

        let instantiate_msg = InstantiateMsg {
            name: "TaxToken".to_string(),
            symbol: "TAX".to_string(),
            decimals: 6,
            cw20_code_id: 1,
            initial_balances: vec![cw20::Cw20Coin {
                address: addrs.user_a.to_string(),
                amount: Uint128::new(1_000_000),
            }],
            admin: addrs.admin.to_string(),
            router: "router_address".to_string(),
            mint: None,
            marketing: None,
        };

        let info = message_info(&addrs.admin, &[]);
        let _res = instantiate(deps.as_mut(), env.clone(), info, instantiate_msg).unwrap();
        TREASURY
            .save(deps.as_mut().storage, &addrs.treasury.to_string())
            .unwrap();

        let tax_info = message_info(&addrs.admin, &[]);
        let tax_msg = ExecuteMsg::SetTaxRate {
            global_rate: Decimal::percent(10),
            reflection_rate: Decimal::zero(),
            burn_rate: Decimal::zero(),
            antiwhale_rate: Decimal::one(),
        };
        execute(deps.as_mut(), env.clone(), tax_info, tax_msg).unwrap();

        (deps, env, addrs)
    }

    // --- REVISED: The query_balance helper is much simpler ---
    // It just calls our own contract's query entrypoint, which is the most accurate way to test.
    fn query_balance(deps: &cosmwasm_std::DepsMut, address: &Addr) -> Uint128 {
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::Balance {
                address: address.to_string(),
            },
        )
        .unwrap();
        let balance: BalanceResponse = from_json(&res).unwrap();
        balance.balance
    }

    #[test]
    fn test_aggregator_management() {
        let (mut deps, env, addrs) = setup_test();
        let admin_info = message_info(&addrs.admin, &[]);
        let user_info = message_info(&addrs.user_a, &[]);
        let add_msg = ExecuteMsg::AddAggregator {
            address: addrs.aggregator.to_string(),
        };
        let res = execute(
            deps.as_mut(),
            env.clone(),
            user_info.clone(),
            add_msg.clone(),
        );
        assert!(res.is_err());
        execute(deps.as_mut(), env.clone(), admin_info.clone(), add_msg).unwrap();
        let remove_msg = ExecuteMsg::RemoveAggregator {
            address: addrs.aggregator.to_string(),
        };
        let res = execute(deps.as_mut(), env.clone(), user_info, remove_msg.clone());
        assert!(res.is_err());
        execute(deps.as_mut(), env, admin_info, remove_msg).unwrap();
    }

    #[test]
    fn test_tax_exempt_transfer_and_send() {
        let (mut deps, env, addrs) = setup_test();

        // Arrange: Register the aggregator and give it some funds.
        let admin_info = message_info(&addrs.admin, &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::AddAggregator {
                address: addrs.aggregator.to_string(),
            },
        )
        .unwrap();

        let user_a_info = message_info(&addrs.user_a, &[]);
        let transfer_to_agg_msg = ExecuteMsg::Transfer {
            recipient: addrs.aggregator.to_string(),
            amount: Uint128::new(100_000),
        };
        execute(deps.as_mut(), env.clone(), user_a_info, transfer_to_agg_msg).unwrap();

        assert_eq!(
            query_balance(&deps.as_mut(), &addrs.aggregator),
            Uint128::new(90_000)
        );

        // --- Test TaxExemptTransfer (to User B) ---
        let aggregator_info = message_info(&addrs.aggregator, &[]);
        let exempt_transfer_msg = ExecuteMsg::TaxExemptTransfer {
            recipient: addrs.user_b.to_string(),
            amount: Uint128::new(50_000),
        };

        let res = execute(
            deps.as_mut(),
            env.clone(),
            aggregator_info.clone(),
            exempt_transfer_msg,
        )
        .unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(
            query_balance(&deps.as_mut(), &addrs.user_b),
            Uint128::new(50_000)
        );

        // --- Test TaxExemptSend (to Pair) ---
        let hook_msg = to_json_binary(&"hook msg").unwrap();
        let exempt_send_msg = ExecuteMsg::TaxExemptSend {
            contract: addrs.pair.to_string(),
            amount: Uint128::new(40_000),
            msg: hook_msg.clone(),
        };

        #[derive(Serialize)]
        #[serde(rename_all = "snake_case")]
        enum ReceiveMsg {
            Receive(Cw20ReceiveMsg),
        }

        let res = execute(deps.as_mut(), env.clone(), aggregator_info, exempt_send_msg).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: addrs.pair.to_string(),
                msg: to_json_binary(&ReceiveMsg::Receive(Cw20ReceiveMsg {
                    sender: addrs.aggregator.to_string(),
                    amount: Uint128::new(40_000),
                    msg: hook_msg,
                }))
                .unwrap(),
                funds: vec![],
            })
        );
        assert_eq!(
            query_balance(&mut deps.as_mut(), &addrs.pair),
            Uint128::new(40_000)
        );
    }

    #[test]
    fn test_post_tax_amount_event_logging() {
        let (mut deps, env, addrs) = setup_test();
        let info = message_info(&addrs.user_a, &[]);
        let msg = ExecuteMsg::Transfer {
            recipient: addrs.user_b.to_string(),
            amount: Uint128::new(100_000),
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        let post_tax_attr = res
            .attributes
            .iter()
            .find(|a| a.key == "post_tax_amount")
            .expect("post_tax_amount attribute not found");
        assert_eq!(post_tax_attr.value, "90000");
        let amount_attr = res
            .attributes
            .iter()
            .find(|a| a.key == "amount")
            .expect("amount attribute not found");
        assert_eq!(amount_attr.value, "90000");
    }

    #[test]
    fn test_tf_recipient_whitelist_management() {
        let (mut deps, env, addrs) = setup_test();
        let admin_info = message_info(&addrs.admin, &[]);
        let user_info = message_info(&addrs.user_a, &[]);
        let add_msg = ExecuteMsg::AddTransferFromRecipient {
            address: addrs.pair.to_string(),
        };
        let res = execute(
            deps.as_mut(),
            env.clone(),
            user_info.clone(),
            add_msg.clone(),
        );
        assert!(res.is_err());
        execute(deps.as_mut(), env.clone(), admin_info, add_msg).unwrap();
    }

    #[test]
    fn test_transfer_from_to_whitelisted_recipient_is_tax_free() {
        let (mut deps, env, addrs) = setup_test();

        let admin_info = message_info(&addrs.admin, &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::AddTransferFromRecipient {
                address: addrs.pair.to_string(),
            },
        )
        .unwrap();

        let user_a_info = message_info(&addrs.user_a, &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            user_a_info,
            ExecuteMsg::IncreaseAllowance {
                spender: addrs.pair.to_string(),
                amount: Uint128::new(100_000),
                expires: None,
            },
        )
        .unwrap();

        let pair_caller_info = message_info(&addrs.pair, &[]);
        let msg = ExecuteMsg::TransferFrom {
            owner: addrs.user_a.to_string(),
            recipient: addrs.pair.to_string(),
            amount: Uint128::new(100_000),
        };
        let res = execute(deps.as_mut(), env, pair_caller_info, msg).unwrap();

        assert_eq!(res.messages.len(), 0);
        assert_eq!(
            query_balance(&deps.as_mut(), &addrs.pair),
            Uint128::new(100_000)
        );
    }

    #[test]
    fn test_regular_transfer_to_tf_whitelisted_recipient_is_still_taxed() {
        let (mut deps, env, addrs) = setup_test();

        let admin_info = message_info(&addrs.admin, &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::AddTransferFromRecipient {
                address: addrs.pair.to_string(),
            },
        )
        .unwrap();

        let user_info = message_info(&addrs.user_a, &[]);
        let msg = ExecuteMsg::Transfer {
            recipient: addrs.pair.to_string(),
            amount: Uint128::new(100_000),
        };
        let res = execute(deps.as_mut(), env, user_info, msg).unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            query_balance(&deps.as_mut(), &addrs.pair),
            Uint128::new(90_000)
        );
    }
}
