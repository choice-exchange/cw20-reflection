use std::ops::{Div, Sub};

use choice::pair::SimulationResponse;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, from_json, to_json_binary, Addr, Api, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, QuerierWrapper, QueryRequest, Response, StdError, StdResult, Storage,
    Uint128, WasmMsg, WasmQuery,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg};

use cw2::set_contract_version;

use crate::msg::{
    Cw20HookMsg, Cw20ReceiveMsg, ExecuteMsg, GetTokenResponse, InstantiateMsg, MigrateMsg,
    QueryMsg, TokenQueryMsg,
};
use choice::asset::{Asset, AssetInfo, PairInfo};
use choice::pair::QueryMsg as PairQueryMsg;
use cw20_base::ContractError;
use cw_storage_plus::Item;

// version info for migration info
const CONTRACT_NAME: &str = "choice:reflection";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const MIN_LIQUIFY_AMT: Item<Uint128> = Item::new("min_liquify_amt"); // minimum number of reflection token before turning into liquidity

pub const ADMIN: Item<String> = Item::new("admin");
pub const TOKEN: Item<Addr> = Item::new("token");
pub const ROUTER: Item<String> = Item::new("router");
pub const LIQUIDITY_TOKEN: Item<String> = Item::new("liquidity_token");
pub const LIQUIDITY_PAIR_CONTRACT: Item<String> = Item::new("liquidity_pair_contract");
pub const REFLECTION_PAIR_CONTRACT: Item<String> = Item::new("reflection_pair_contract");
pub const LIQUIDITY_PAIR: Item<[AssetInfo; 2]> = Item::new("liquidity_pair");
pub const REFLECTION_PAIR: Item<[AssetInfo; 2]> = Item::new("reflection_pair");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    deps.api.addr_validate(&msg.admin.to_string())?;
    deps.api.addr_validate(&msg.router.to_string())?;
    ADMIN.save(deps.storage, &msg.admin.to_string())?;
    ROUTER.save(deps.storage, &msg.router.to_string())?;
    TOKEN.save(deps.storage, &msg.token)?;
    MIN_LIQUIFY_AMT.save(deps.storage, &Uint128::zero())?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => {
            receive_cw20(&deps.querier, deps.storage, deps.api, env, info, msg)
        }
        // Reflection features
        ExecuteMsg::SetReflectionPair {
            asset_infos,
            pair_contract,
        } => set_reflection_pair(deps, env, info, asset_infos, pair_contract),
        ExecuteMsg::SetLiquidityPair {
            asset_infos,
            pair_contract,
        } => set_liquidity_pair(deps, env, info, asset_infos, pair_contract),
        ExecuteMsg::SetMinLiquify { min_liquify_amt } => {
            set_min_liquify_amt(deps, env, info, min_liquify_amt)
        }
        ExecuteMsg::Liquify {} => liquify_treasury(&deps.querier, env, deps.storage),
        ExecuteMsg::WithdrawToken { asset } => withdraw_token(deps, env, info, asset),
        ExecuteMsg::TransferAdmin { new_admin } => transfer_admin(deps, info, new_admin),
    }
}

pub fn transfer_admin(
    deps: DepsMut,
    info: MessageInfo,
    new_admin: String,
) -> Result<Response, ContractError> {
    ensure_admin(&deps, &info)?;
    let new_admin_addr = deps.api.addr_validate(&new_admin)?;
    ADMIN.save(deps.storage, &new_admin_addr.to_string())?;
    Ok(Response::new()
        .add_attribute("action", "transfer_admin")
        .add_attribute("new_admin", new_admin))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance {} => {
            let token: Addr = TOKEN.load(deps.storage)?;
            to_json_binary(&query_balance(&deps.querier, token, env.contract.address)?)
        }
        QueryMsg::GetToken {} => to_json_binary(&query_token(deps.storage)?),
    }
}

pub fn query_token(storage: &dyn Storage) -> StdResult<GetTokenResponse> {
    let token_addr = TOKEN.load(storage)?;
    Ok(GetTokenResponse {
        address: token_addr.to_string(),
    })
}

pub fn receive_cw20(
    querier: &QuerierWrapper,
    storage: &mut dyn Storage,
    _api: &dyn Api,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let token = TOKEN.may_load(storage)?.unwrap();

    match from_json(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Liquify {}) => {
            // only token contract can execute this message
            if token != info.sender {
                return Err(ContractError::Unauthorized {});
            }

            liquify_treasury(querier, env.clone(), storage)
        }
        Err(_) => Err(ContractError::Unauthorized {}),
    }
}

/// Core function of the treasury. Will be used to liquify, burn, and reflect tokens in one operation
/// 1. Liquify reflection token into LP tokens
/// 2. Reflect reflection token into target token to be sent into fee collector wallet
/// 3. Burn a portion of reflection token
pub fn liquify_treasury(
    querier: &QuerierWrapper,
    env: Env,
    storage: &mut dyn Storage,
) -> Result<Response, ContractError> {
    let liquidity_pair = match LIQUIDITY_PAIR.may_load(storage)? {
        Some(pair) => pair,
        None => return Ok(Response::default()), // Exit early
    };
    let liquidity_pair_contract = match LIQUIDITY_PAIR_CONTRACT.may_load(storage)? {
        Some(contract) => contract,
        None => return Ok(Response::default()), // Exit early
    };
    let reflection_pair = match REFLECTION_PAIR.may_load(storage)? {
        Some(pair) => pair,
        None => return Ok(Response::default()), // Exit early
    };
    let router = match ROUTER.may_load(storage)? {
        Some(r) => r,
        None => return Ok(Response::default()), // Exit early
    };

    let querier = *querier;

    // let admin = ADMIN.may_load(storage)?.unwrap_or_default();
    let token = TOKEN.load(storage)?;
    let contract_balance = query_balance(&querier, token.clone(), env.contract.address.clone())?;

    let min_liquify_amt = MIN_LIQUIFY_AMT
        .may_load(storage)?
        .unwrap_or(Uint128::zero());

    // Short circuit if there's not enough contract balance to liquify
    if contract_balance < min_liquify_amt {
        return Ok(Response::default());
    }

    // Loads all the tax rates from the modified CW20 token
    let (_tax_rate, reflection_rate, burn_rate, _transfer_rate): (
        Decimal,
        Decimal,
        Decimal,
        Decimal,
    ) = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: token.to_string(),
        msg: to_json_binary(&TokenQueryMsg::QueryRates {})?,
    }))?;

    let mut messages: Vec<WasmMsg> = vec![];

    let reflect_amt = contract_balance.mul_floor(reflection_rate);
    let burn_amt = contract_balance.mul_floor(burn_rate);

    let liquidity_amt = contract_balance.sub(reflect_amt).sub(burn_amt);
    // Taxes - 100000
    // Reflection - 50000
    // Burn - 10000
    // Liq amt - 40000
    if liquidity_amt > Uint128::zero() {
        // Swaps half of reflection token into INJ
        let swap_amount = liquidity_amt.div(Uint128::from(2u128));
        // Increases allowance of reflection token to liquidity pair contract (allows adding liquidity)
        messages.push(WasmMsg::Execute {
            contract_addr: token.to_string(),
            msg: to_json_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
                spender: liquidity_pair_contract.clone(),
                amount: liquidity_amt.sub(swap_amount),
                expires: None,
            })?,
            funds: vec![],
        });

        // Simulates swapping of half of reflection token into INJ
        let simulation = simulate(
            &querier,
            liquidity_pair_contract.clone(),
            &Asset {
                amount: swap_amount,
                info: liquidity_pair[0].clone(),
            },
        )?;
        // We formulate a swap message to swap reflection token into INJ
        messages.push(WasmMsg::Execute {
            contract_addr: token.to_string(),
            msg: to_json_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: liquidity_pair_contract.to_string(),
                amount: swap_amount,
                msg: to_json_binary(&choice::pair::Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                    deadline: None,
                })?,
            })?,
            funds: vec![],
        });

        // Formulate variable to allow us to add liquidity to the pool
        let assets: [Asset; 2] = [
            Asset {
                amount: liquidity_amt.sub(swap_amount), // add remaining amount of reflection token as liquidity
                info: liquidity_pair[0].clone(),        // reflection token
            },
            Asset {
                amount: simulation.return_amount, // add simulated INJ return amount to be added as liquidity
                info: liquidity_pair[1].clone(),  // INJ
            },
        ];

        // We formulate a ProvideLiquidity message to add reflection token liquidity to the pool
        match reflection_pair[1].clone() {
            AssetInfo::NativeToken { denom } => {
                // If the asset is a native token, we provide liquidity via a denom message
                messages.push(WasmMsg::Execute {
                    contract_addr: liquidity_pair_contract.to_string(),
                    msg: to_json_binary(&choice::pair::ExecuteMsg::ProvideLiquidity {
                        assets,
                        receiver: None,
                        deadline: None,
                        slippage_tolerance: None,
                    })?,
                    funds: vec![coin(simulation.return_amount.u128(), denom)],
                });
            }
            AssetInfo::Token { contract_addr } => {
                // If asset is a CW20, we provide liquidity via increase allowance message
                messages.push(WasmMsg::Execute {
                    contract_addr,
                    msg: to_json_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                        spender: liquidity_pair_contract.to_string(),
                        amount: simulation.return_amount,
                        expires: None,
                    })?,
                    funds: vec![],
                });
                messages.push(WasmMsg::Execute {
                    contract_addr: liquidity_pair_contract.to_string(),
                    msg: to_json_binary(&choice::pair::ExecuteMsg::ProvideLiquidity {
                        assets,
                        receiver: None,
                        deadline: None,
                        slippage_tolerance: None,
                    })?,
                    funds: vec![],
                });
            }
        };
    }

    if reflect_amt > Uint128::zero() {
        let self_token_info = AssetInfo::Token {
            contract_addr: token.to_string(),
        };
        // 1. Define the first swap operation, which is always required.
        // This swaps your reflection token into the intermediate asset (e.g., INJ).
        let first_op = choice::router::SwapOperation::Choice {
            offer_asset_info: choice::asset::AssetInfo::Token {
                contract_addr: token.to_string(),
            },
            ask_asset_info: reflection_pair[1].clone(),
        };

        // 2. Start with a mutable vector containing only the first operation.
        let mut operations = vec![first_op];

        // 3. Only add the second swap if the final target is a different token.
        if reflection_pair[0] != self_token_info {
            // 4. If they are different, add the second swap operation.
            // This swaps the intermediate asset (e.g., INJ) into the final reward (e.g., DOJO).
            let second_op = choice::router::SwapOperation::Choice {
                offer_asset_info: reflection_pair[1].clone(),
                ask_asset_info: reflection_pair[0].clone(),
            };
            operations.push(second_op);
        }

        // 5. Execute the swap(s). The `operations` vector now contains either one or two steps.
        messages.push(WasmMsg::Execute {
            contract_addr: token.to_string(),
            msg: to_json_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: router.to_string(),
                amount: reflect_amt,
                msg: to_json_binary(&choice::router::ExecuteMsg::ExecuteSwapOperations {
                    operations,
                    minimum_receive: None,
                    to: None, // target token is sent here into treasury
                    deadline: None,
                })?,
            })?,
            funds: vec![],
        });
    }

    if burn_amt > Uint128::zero() {
        // Burn
        messages.push(WasmMsg::Execute {
            contract_addr: token.to_string(),
            msg: to_json_binary(&cw20::Cw20ExecuteMsg::Burn { amount: burn_amt })?,
            funds: vec![],
        });
    }

    let res = Response::new().add_messages(messages);

    Ok(res)
}

/// Used to simulate swap operations against choice pair
pub fn simulate(
    querier: &QuerierWrapper,
    pair_contract: String,
    offer_asset: &Asset,
) -> StdResult<SimulationResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract,
        msg: to_json_binary(&PairQueryMsg::Simulation {
            offer_asset: offer_asset.clone(),
        })?,
    }))
}

pub fn query_balance(querier: &QuerierWrapper, token: Addr, address: Addr) -> StdResult<Uint128> {
    let response: BalanceResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: token.to_string(),
        msg: to_json_binary(&cw20::Cw20QueryMsg::Balance {
            address: address.to_string(),
        })?,
    }))?;
    Ok(response.balance)
}

// Check below for pair ordering
// 1. This contract address (reflection token)
// 2. The quote token (inj)
pub fn set_liquidity_pair(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    asset_infos: [AssetInfo; 2],
    pair_contract: String,
) -> Result<Response, ContractError> {
    ensure_admin(&deps, &info)?;
    let reflection_pair = REFLECTION_PAIR.load(deps.storage);
    LIQUIDITY_PAIR.save(deps.storage, &asset_infos)?;
    LIQUIDITY_PAIR_CONTRACT.save(deps.storage, &pair_contract)?;

    match reflection_pair {
        Err(_) => {}
        Ok(asset_info) => {
            let unbound = asset_info;
            let reflect_1 = unbound.get(1).unwrap();
            if !reflect_1.eq(&asset_infos[1]) {
                return Err(ContractError::Std(StdError::generic_err(
                    "asset_infos[1] do not match",
                )));
            }
        }
    };

    let response: PairInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract,
        msg: to_json_binary(&PairQueryMsg::Pair {})?,
    }))?;

    match response.asset_infos[0].clone() {
        AssetInfo::Token { contract_addr } => {
            deps.api.addr_validate(&contract_addr.to_string())?;
        }
        AssetInfo::NativeToken { denom: _ } => {
            return Err(ContractError::Std(StdError::generic_err(
                "token should be cw20",
            )));
        }
    };

    LIQUIDITY_TOKEN.save(deps.storage, &response.liquidity_token.to_string())?;

    response
        .asset_infos
        .iter()
        .find(|info| info.equal(&asset_infos[0]))
        .ok_or(StdError::generic_err("asset_infos[0] is not valid"))?;

    response
        .asset_infos
        .iter()
        .find(|info| info.equal(&asset_infos[1]))
        .ok_or(StdError::generic_err("asset_infos[1] is not valid"))?;

    Ok(Response::default())
}

// Check below for pair ordering
// 1. The target token (DOJO)
// 2. The quote token (inj)
pub fn set_reflection_pair(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    asset_infos: [AssetInfo; 2],
    pair_contract: String,
) -> Result<Response, ContractError> {
    ensure_admin(&deps, &info)?;
    let liquidity_pair = LIQUIDITY_PAIR.load(deps.storage);
    REFLECTION_PAIR.save(deps.storage, &asset_infos)?;
    REFLECTION_PAIR_CONTRACT.save(deps.storage, &pair_contract)?;

    match liquidity_pair {
        Err(_) => {}
        Ok(asset_info) => {
            let unbound = asset_info;
            let liquidity_1 = unbound.get(1).unwrap();
            if !liquidity_1.eq(&asset_infos[1]) {
                return Err(ContractError::Std(StdError::generic_err(
                    "asset_infos[1] do not match",
                )));
            }
        }
    };

    let response: PairInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract,
        msg: to_json_binary(&PairQueryMsg::Pair {})?,
    }))?;

    response
        .asset_infos
        .iter()
        .find(|info| info.equal(&asset_infos[0]))
        .ok_or(StdError::generic_err("asset_infos[0] is not valid"))?;

    response
        .asset_infos
        .iter()
        .find(|info| info.equal(&asset_infos[1]))
        .ok_or(StdError::generic_err("asset_infos[1] is not valid"))?;

    Ok(Response::default())
}

/// Sets minimum reflection token required to liquify
pub fn set_min_liquify_amt(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    min_liquify_amt: Uint128,
) -> Result<Response, ContractError> {
    ensure_admin(&deps, &info)?;

    MIN_LIQUIFY_AMT.save(deps.storage, &min_liquify_amt)?;
    Ok(Response::default())
}

/// Withdraws a CW20 or Native token of your choice from the contract.
/// It is not allowed to withdraw the LP token itself.
pub fn withdraw_token(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset: AssetInfo,
) -> Result<Response, ContractError> {
    ensure_admin(&deps, &info)?;

    // Load the LP token address from storage. Based on Choice, this will be a CW20 address.
    let lp_token_addr = LIQUIDITY_TOKEN.may_load(deps.storage)?.unwrap_or_default();
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut response = Response::new();

    match asset {
        AssetInfo::Token { contract_addr } => {
            // --- CW20 TOKEN LOGIC ---

            // Prevents the LP token from being withdrawn
            if contract_addr == lp_token_addr {
                return Err(ContractError::Std(StdError::generic_err(
                    "Unauthorized: not allowed to withdraw LP token",
                )));
            }

            // Query the balance of the CW20 token
            let balance: cw20::BalanceResponse = deps.querier.query_wasm_smart(
                contract_addr.clone(),
                &cw20::Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                },
            )?;

            if balance.balance.is_zero() {
                return Err(ContractError::Std(StdError::generic_err(
                    "No CW20 balance to withdraw",
                )));
            }

            // Create a CW20 transfer message
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount: balance.balance,
                })?,
                funds: vec![],
            }));

            response = response.add_attribute("withdraw_cw20_token", contract_addr.to_string());
            response = response.add_attribute("withdraw_amount", balance.balance);
        }
        AssetInfo::NativeToken { denom } => {
            // --- NATIVE TOKEN LOGIC ---

            // Query the contract's native balance for the specified denomination
            let balance = deps
                .querier
                .query_balance(env.contract.address, denom.clone())?;

            if balance.amount.is_zero() {
                return Err(ContractError::Std(StdError::generic_err(
                    "No native balance to withdraw",
                )));
            }

            // Create a BankMsg to send the native coins to the admin
            messages.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: vec![balance.clone()],
            }));

            response = response.add_attribute("withdraw_native_token", balance.denom);
            response = response.add_attribute("withdraw_amount", balance.amount);
        }
    }

    Ok(response
        .add_messages(messages)
        .add_attribute("action", "withdraw_token"))
}

/// Ensures only admins can use this function
pub fn ensure_admin(deps: &DepsMut, info: &MessageInfo) -> Result<Response, ContractError> {
    let admin = ADMIN.may_load(deps.storage)?.unwrap_or_default();
    if info.sender.to_string() != admin {
        return Err(ContractError::Std(StdError::generic_err(
            "Unauthorized: not admin",
        )));
    }

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
