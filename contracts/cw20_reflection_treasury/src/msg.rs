use choice::asset::AssetInfo;
use cosmwasm_std::Addr;
use cosmwasm_std::Binary;
use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct InstantiateMsg {
    pub admin: String,
    pub router: String,
    pub token: Addr,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    SetLiquidityPair {
        asset_infos: [AssetInfo; 2],
        pair_contract: String,
    },
    SetReflectionPair {
        asset_infos: [AssetInfo; 2],
        pair_contract: String,
    },
    SetMinLiquify {
        min_liquify_amt: Uint128,
    },
    WithdrawToken {
        asset: AssetInfo,
    },
    Liquify {},
    TransferAdmin {
        new_admin: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Balance {},
    GetToken {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Default)]
pub struct QueryTaxResponse {
    pub taxed_amount: Uint128,
    pub after_tax: Uint128,
    pub reflection_amount: Uint128,
    pub liquidity_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetTokenResponse {
    pub address: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Default)]
pub struct MigrateMsg {
    pub msg: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TokenQueryMsg {
    QueryRates {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Cw20ReceiveMsg {
    pub sender: String,
    pub amount: Uint128,
    pub msg: Binary,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Liquify {},
}
