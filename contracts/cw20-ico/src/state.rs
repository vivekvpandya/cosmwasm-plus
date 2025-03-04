use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use cosmwasm_std::{CanonicalAddr, HumanAddr, Uint128};
use cw_storage_plus::{Item, Map};

use cw20::AllowanceResponse;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
    pub mint: Option<MinterData>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct MinterData {
    pub minter: CanonicalAddr,
    /// cap is how many more tokens can be issued by the minter
    pub cap: Option<Uint128>,
}

impl TokenInfo {
    pub fn get_cap(&self) -> Option<Uint128> {
        self.mint.as_ref().and_then(|v| v.cap)
    }
}


#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct IcoInfo {
  pub owner : CanonicalAddr,
  pub target_amount: Uint128,
  pub raised_amount: Uint128,
  pub conversion_ratio_norm: Uint128,
  pub conversion_ratio_denorm: Uint128,
}

pub const ICO_INFO : Item<IcoInfo> = Item::new("ico_info");
pub const TOKEN_INFO: Item<TokenInfo> = Item::new("token_info");
pub const BALANCES: Map<&[u8], Uint128> = Map::new("balance");
pub const NEW_TOKEN_BALANCES: Map<&[u8], Uint128> = Map::new("new_balance");
pub const INVESTORS: Map<&[u8], Uint128> = Map::new("investors");
pub const ALLOWANCES: Map<(&[u8], &[u8]), AllowanceResponse> = Map::new("allowance");
