#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Binary, Deps, DepsMut, Env, HumanAddr, MessageInfo, Response, StdError,
    StdResult, Uint128, CosmosMsg, Order
};

use cw2::set_contract_version;
use cw20::{BalanceResponse, Cw20CoinHuman, Cw20ReceiveMsg, MinterResponse, TokenInfoResponse};

use crate::allowances::{
    execute_burn_from, execute_decrease_allowance, execute_increase_allowance, execute_send_from,
    execute_transfer_from, query_allowance,
};
use crate::enumerable::{query_all_accounts, query_all_allowances};
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, InstantiateIcoMsg, QueryMsg};
use crate::state::{MinterData, TokenInfo, IcoInfo, INVESTORS, BALANCES, NEW_TOKEN_BALANCES, TOKEN_INFO, ICO_INFO};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-base";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateIcoMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let total_supply = create_accounts(&mut deps, &msg.initial_balances)?;
    if msg.target_amount > total_supply {

        return Err(StdError::generic_err("ICO cannot raise more than total-supply"));
    }
    let data = IcoInfo {
        owner : deps.api.canonical_address(&msg.owner)? ,
        target_amount: msg.target_amount,
        raised_amount: Uint128(0),
        conversion_ratio_norm: msg.conversion_ratio_norm,
        conversion_ratio_denorm: msg.conversion_ratio_denorm,
    };
    ICO_INFO.save(deps.storage, &data)?;
    Ok(Response::default())
}

// #[cfg_attr(not(feature = "library"), entry_point)]
// pub fn instantiate(
//     mut deps: DepsMut,
//     _env: Env,
//     _info: MessageInfo,
//     msg: InstantiateMsg,
// ) -> StdResult<Response> {
//     set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
//     // check valid token info
//     msg.validate()?;
//     // create initial accounts
//     let total_supply = create_accounts(&mut deps, &msg.initial_balances)?;

//     if let Some(limit) = msg.get_cap() {
//         if total_supply > limit {
//             return Err(StdError::generic_err("Initial supply greater than cap"));
//         }
//     }

//     let mint = match msg.mint {
//         Some(m) => Some(MinterData {
//             minter: deps.api.canonical_address(&m.minter)?,
//             cap: m.cap,
//         }),
//         None => None,
//     };

//     // store token info
//     let data = TokenInfo {
//         name: msg.name,
//         symbol: msg.symbol,
//         decimals: msg.decimals,
//         total_supply,
//         mint,
//     };
//     TOKEN_INFO.save(deps.storage, &data)?;
//     Ok(Response::default())
// }

pub fn create_accounts(deps: &mut DepsMut, accounts: &[Cw20CoinHuman]) -> StdResult<Uint128> {
    let mut total_supply = Uint128::zero();
    for row in accounts {
        let raw_address = deps.api.canonical_address(&row.address)?;
        BALANCES.save(deps.storage, &raw_address, &row.amount)?;
        NEW_TOKEN_BALANCES.save(deps.storage, &raw_address, &Uint128(0))?;
        total_supply += row.amount;
    }
    Ok(total_supply)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => execute_send(deps, env, info, contract, amount, msg),
        ExecuteMsg::Mint { recipient, amount } => execute_mint(deps, env, info, recipient, amount),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        ExecuteMsg::BurnFrom { owner, amount } => execute_burn_from(deps, env, info, owner, amount),
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => execute_send_from(deps, env, info, owner, contract, amount, msg),
        ExecuteMsg::InvestInIco { ico, amount } => execute_invest_ico(
            deps, env, info, ico, amount
        ),
        ExecuteMsg::CloseIco { ico } => execute_close_ico(deps, env, info),
    }
}

pub fn execute_close_ico(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let sender_raw = deps.api.canonical_address(&info.sender)?;
    let mut config = ICO_INFO.load(deps.storage)?;
    if config.owner != sender_raw  {
        // only owner can send closeico message
        return Err(ContractError::Unauthorized{});
    }
    if config.target_amount > config.raised_amount {
        // return fund to investors as this is emergengy
        let all: StdResult<Vec<_>> = INVESTORS.range(deps.storage, None, None, Order::Ascending).collect();
        let all = all.unwrap();
        for (addr, money) in all {
            BALANCES.update(
                    deps.storage,
                    &addr,
                    |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + money) },
                    )?;
                    // decrease money from sender (which is owner of ICO)
            BALANCES.update(
                    deps.storage,
                    &sender_raw,
                    |balance: Option<Uint128>| -> StdResult<_> { balance.unwrap_or_default() - money },
                    )?;
        }
    } else {
        allocate_tokens(deps, _env)?;
    }
    let res = Response {
        submessages: vec![],
        messages : vec![],
        // messages: vec![CosmosMsg::Custom("ICO closed")],
        attributes: vec![],
        data: None,
    };
    Ok(res)
}

pub fn allocate_tokens(
    deps: DepsMut,
    _env: Env,
) -> Result<Response, ContractError> {
    let mut config = ICO_INFO.load(deps.storage)?;
    let norm = config.conversion_ratio_norm;
    let denorm = config.conversion_ratio_denorm;
    let all: StdResult<Vec<_>> = INVESTORS.range(deps.storage, None, None, Order::Ascending).collect();
    let all = all.unwrap();
    for (addr, money) in all {
        let asset =  money.multiply_ratio(norm, denorm);
        NEW_TOKEN_BALANCES.update(
                deps.storage,
                &addr,
                |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + asset) },
                )?;
    }
    let res = Response {
        submessages: vec![],
        messages : vec![],
        // messages: vec![CosmosMsg::Custom("Allocation completed!")],
        attributes: vec![],
        data: None,
    };
    Ok(res)
}

pub fn execute_invest_ico(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    ico: HumanAddr,
    amount: Uint128,
) -> Result<Response, ContractError> {

    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut config = ICO_INFO.load(deps.storage)?;
    let owner_raw = deps.api.canonical_address(&ico)?;
    if config.owner != owner_raw {
        return Err(ContractError::WrongIcoAddress{});
    }
    let investor_raw = deps.api.canonical_address(&info.sender)?;
    let mut in_amount = amount;
    let mut close_ico = false;
    if amount >= (config.target_amount - config.raised_amount)? {
        in_amount = (config.target_amount - config.raised_amount)?;
        close_ico = true;
    }
    // update raised_amount
    ICO_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.raised_amount = (info.raised_amount + in_amount);
        Ok(info)
    })?;
    BALANCES.update(deps.storage, &investor_raw, |balance: Option<Uint128>| {
        balance.unwrap_or_default() - in_amount
    })?;
    BALANCES.update(
        deps.storage,
        &owner_raw,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + in_amount) },
    )?;
// track amount invested , if ICO closes in emergency use this map to return funds
    INVESTORS.update(deps.storage, &investor_raw, |balance: Option<Uint128>| -> StdResult<_> {
       Ok(balance.unwrap_or_default() + in_amount)
    })?;
    let res = Response {
        submessages: vec![],
        messages: vec![],
        attributes: vec![
            attr("action", "ICO Investment"),
            attr("from", deps.api.human_address(&investor_raw)?),
            attr("to", ico),
            attr("amount", in_amount),
        ],
        data: None,
    };
    //TODO: clsoe ICO
    Ok(res)
}

pub fn execute_transfer(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: HumanAddr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_raw = deps.api.canonical_address(&recipient)?;
    let sender_raw = deps.api.canonical_address(&info.sender)?;

    BALANCES.update(deps.storage, &sender_raw, |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    BALANCES.update(
        deps.storage,
        &rcpt_raw,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;


    let res = Response {
        submessages: vec![],
        messages: vec![],
        attributes: vec![
            attr("action", "transfer"),
            attr("from", deps.api.human_address(&sender_raw)?),
            attr("to", recipient),
            attr("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn execute_burn(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let sender_raw = deps.api.canonical_address(&info.sender)?;

    // lower balance
    BALANCES.update(deps.storage, &sender_raw, |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    // reduce total_supply
    TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = (info.total_supply - amount)?;
        Ok(info)
    })?;

    let res = Response {
        submessages: vec![],
        messages: vec![],
        attributes: vec![
            attr("action", "burn"),
            attr("from", deps.api.human_address(&sender_raw)?),
            attr("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn execute_mint(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: HumanAddr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut config = TOKEN_INFO.load(deps.storage)?;
    if config.mint.is_none()
        || config.mint.as_ref().unwrap().minter != deps.api.canonical_address(&info.sender)?
    {
        return Err(ContractError::Unauthorized {});
    }

    // update supply and enforce cap
    config.total_supply += amount;
    if let Some(limit) = config.get_cap() {
        if config.total_supply > limit {
            return Err(ContractError::CannotExceedCap {});
        }
    }
    TOKEN_INFO.save(deps.storage, &config)?;

    // add amount to recipient balance
    let rcpt_raw = deps.api.canonical_address(&recipient)?;
    BALANCES.update(
        deps.storage,
        &rcpt_raw,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response {
        submessages: vec![],
        messages: vec![],
        attributes: vec![
            attr("action", "mint"),
            attr("to", recipient),
            attr("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn execute_send(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    contract: HumanAddr,
    amount: Uint128,
    msg: Option<Binary>,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_raw = deps.api.canonical_address(&contract)?;
    let sender_raw = deps.api.canonical_address(&info.sender)?;

    // move the tokens to the contract
    BALANCES.update(deps.storage, &sender_raw, |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    BALANCES.update(
        deps.storage,
        &rcpt_raw,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let sender = deps.api.human_address(&sender_raw)?;
    let attrs = vec![
        attr("action", "send"),
        attr("from", &sender),
        attr("to", &contract),
        attr("amount", amount),
    ];

    // create a send message
    let msg = Cw20ReceiveMsg {
        sender,
        amount,
        msg,
    }
    .into_cosmos_msg(contract)?;

    let res = Response {
        submessages: vec![],
        messages: vec![msg],
        attributes: attrs,
        data: None,
    };
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        QueryMsg::Minter {} => to_binary(&query_minter(deps)?),
        QueryMsg::Allowance { owner, spender } => {
            to_binary(&query_allowance(deps, owner, spender)?)
        }
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => to_binary(&query_all_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllAccounts { start_after, limit } => {
            to_binary(&query_all_accounts(deps, start_after, limit)?)
        }
    }
}

pub fn query_balance(deps: Deps, address: HumanAddr) -> StdResult<BalanceResponse> {
    let addr_raw = deps.api.canonical_address(&address)?;
    let balance = BALANCES
        .may_load(deps.storage, &addr_raw)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

pub fn query_token_info(deps: Deps) -> StdResult<TokenInfoResponse> {
    let info = TOKEN_INFO.load(deps.storage)?;
    let res = TokenInfoResponse {
        name: info.name,
        symbol: info.symbol,
        decimals: info.decimals,
        total_supply: info.total_supply,
    };
    Ok(res)
}

pub fn query_minter(deps: Deps) -> StdResult<Option<MinterResponse>> {
    let meta = TOKEN_INFO.load(deps.storage)?;
    let minter = match meta.mint {
        Some(m) => Some(MinterResponse {
            minter: deps.api.human_address(&m.minter)?,
            cap: m.cap,
        }),
        None => None,
    };
    Ok(minter)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary, CosmosMsg, StdError, WasmMsg};

    use super::*;

    fn get_balance<T: Into<HumanAddr>>(deps: Deps, address: T) -> Uint128 {
        query_balance(deps, address.into()).unwrap().balance
    }

    // this will set up the instantiation for other tests
    fn do_instantiate_with_minter(
        deps: DepsMut,
        addr: &HumanAddr,
        amount: Uint128,
        minter: &HumanAddr,
        cap: Option<Uint128>,
    ) -> TokenInfoResponse {
        _do_instantiate(
            deps,
            addr,
            amount,
            Some(MinterResponse {
                minter: minter.into(),
                cap,
            }),
        )
    }

    // this will set up the instantiation for other tests
    fn do_instantiate(deps: DepsMut, addr: &HumanAddr, amount: Uint128) -> TokenInfoResponse {
        _do_instantiate(deps, addr, amount, None)
    }

    // this will set up the instantiation for other tests
    fn _do_instantiate(
        mut deps: DepsMut,
        addr: &HumanAddr,
        amount: Uint128,
        mint: Option<MinterResponse>,
    ) -> TokenInfoResponse {
        let instantiate_msg = InstantiateMsg {
            name: "Auto Gen".to_string(),
            symbol: "AUTO".to_string(),
            decimals: 3,
            initial_balances: vec![Cw20CoinHuman {
                address: addr.into(),
                amount,
            }],
            mint: mint.clone(),
        };
        let info = mock_info(&HumanAddr("creator".to_string()), &[]);
        let env = mock_env();
        let res = instantiate(deps.branch(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        let meta = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(
            meta,
            TokenInfoResponse {
                name: "Auto Gen".to_string(),
                symbol: "AUTO".to_string(),
                decimals: 3,
                total_supply: amount,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), addr), amount);
        assert_eq!(query_minter(deps.as_ref()).unwrap(), mint,);
        meta
    }

    #[test]
    fn proper_instantiation() {
        let mut deps = mock_dependencies(&[]);
        let amount = Uint128::from(11223344u128);
        let instantiate_msg = InstantiateMsg {
            name: "Cash Token".to_string(),
            symbol: "CASH".to_string(),
            decimals: 9,
            initial_balances: vec![Cw20CoinHuman {
                address: HumanAddr("addr0000".to_string()),
                amount,
            }],
            mint: None,
        };
        let info = mock_info(&HumanAddr("creator".to_string()), &[]);
        let env = mock_env();
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        assert_eq!(
            query_token_info(deps.as_ref()).unwrap(),
            TokenInfoResponse {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                total_supply: amount,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), "addr0000"), Uint128(11223344));
    }

    #[test]
    fn instantiate_mintable() {
        let mut deps = mock_dependencies(&[]);
        let amount = Uint128(11223344);
        let minter = HumanAddr::from("asmodat");
        let limit = Uint128(511223344);
        let instantiate_msg = InstantiateMsg {
            name: "Cash Token".to_string(),
            symbol: "CASH".to_string(),
            decimals: 9,
            initial_balances: vec![Cw20CoinHuman {
                address: HumanAddr("addr0000".to_string()),
                amount,
            }],
            mint: Some(MinterResponse {
                minter: minter.clone(),
                cap: Some(limit),
            }),
        };
        let info = mock_info(&HumanAddr("creator".to_string()), &[]);
        let env = mock_env();
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        assert_eq!(
            query_token_info(deps.as_ref()).unwrap(),
            TokenInfoResponse {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                total_supply: amount,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), "addr0000"), Uint128(11223344));
        assert_eq!(
            query_minter(deps.as_ref()).unwrap(),
            Some(MinterResponse {
                minter: minter.clone(),
                cap: Some(limit),
            }),
        );
    }

    #[test]
    fn instantiate_mintable_over_cap() {
        let mut deps = mock_dependencies(&[]);
        let amount = Uint128(11223344);
        let minter = HumanAddr::from("asmodat");
        let limit = Uint128(11223300);
        let instantiate_msg = InstantiateMsg {
            name: "Cash Token".to_string(),
            symbol: "CASH".to_string(),
            decimals: 9,
            initial_balances: vec![Cw20CoinHuman {
                address: HumanAddr("addr0000".to_string()),
                amount,
            }],
            mint: Some(MinterResponse {
                minter: minter.clone(),
                cap: Some(limit),
            }),
        };
        let info = mock_info(&HumanAddr("creator".to_string()), &[]);
        let env = mock_env();
        let err =
            instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err("Initial supply greater than cap")
        );
    }

    #[test]
    fn can_mint_by_minter() {
        let mut deps = mock_dependencies(&[]);

        let genesis = HumanAddr::from("genesis");
        let amount = Uint128(11223344);
        let minter = HumanAddr::from("asmodat");
        let limit = Uint128(511223344);
        do_instantiate_with_minter(deps.as_mut(), &genesis, amount, &minter, Some(limit));

        // minter can mint coins to some winner
        let winner = HumanAddr::from("lucky");
        let prize = Uint128(222_222_222);
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: prize,
        };

        let info = mock_info(&minter, &[]);
        let env = mock_env();
        let res = execute(deps.as_mut(), env, info, msg.clone()).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(get_balance(deps.as_ref(), &genesis), amount);
        assert_eq!(get_balance(deps.as_ref(), &winner), prize);

        // but cannot mint nothing
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: Uint128::zero(),
        };
        let info = mock_info(&minter, &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // but if it exceeds cap (even over multiple rounds), it fails
        // cap is enforced
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: Uint128(333_222_222),
        };
        let info = mock_info(&minter, &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::CannotExceedCap {});
    }

    #[test]
    fn others_cannot_mint() {
        let mut deps = mock_dependencies(&[]);
        do_instantiate_with_minter(
            deps.as_mut(),
            &HumanAddr::from("genesis"),
            Uint128(1234),
            &HumanAddr::from("minter"),
            None,
        );

        let msg = ExecuteMsg::Mint {
            recipient: HumanAddr::from("lucky"),
            amount: Uint128(222),
        };
        let info = mock_info(&HumanAddr::from("anyone else"), &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn no_one_mints_if_minter_unset() {
        let mut deps = mock_dependencies(&[]);
        do_instantiate(deps.as_mut(), &HumanAddr::from("genesis"), Uint128(1234));

        let msg = ExecuteMsg::Mint {
            recipient: HumanAddr::from("lucky"),
            amount: Uint128(222),
        };
        let info = mock_info(&HumanAddr::from("genesis"), &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn instantiate_multiple_accounts() {
        let mut deps = mock_dependencies(&[]);
        let amount1 = Uint128::from(11223344u128);
        let addr1 = HumanAddr::from("addr0001");
        let amount2 = Uint128::from(7890987u128);
        let addr2 = HumanAddr::from("addr0002");
        let instantiate_msg = InstantiateMsg {
            name: "Bash Shell".to_string(),
            symbol: "BASH".to_string(),
            decimals: 6,
            initial_balances: vec![
                Cw20CoinHuman {
                    address: addr1.clone(),
                    amount: amount1,
                },
                Cw20CoinHuman {
                    address: addr2.clone(),
                    amount: amount2,
                },
            ],
            mint: None,
        };
        let info = mock_info(&HumanAddr("creator".to_string()), &[]);
        let env = mock_env();
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        assert_eq!(
            query_token_info(deps.as_ref()).unwrap(),
            TokenInfoResponse {
                name: "Bash Shell".to_string(),
                symbol: "BASH".to_string(),
                decimals: 6,
                total_supply: amount1 + amount2,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), &addr1), amount1);
        assert_eq!(get_balance(deps.as_ref(), &addr2), amount2);
    }

    #[test]
    fn queries_work() {
        let mut deps = mock_dependencies(&coins(2, "token"));
        let addr1 = HumanAddr::from("addr0001");
        let amount1 = Uint128::from(12340000u128);

        let expected = do_instantiate(deps.as_mut(), &addr1, amount1);

        // check meta query
        let loaded = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(expected, loaded);

        let _info = mock_info("test", &[]);
        let env = mock_env();
        // check balance query (full)
        let data = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::Balance {
                address: addr1.clone(),
            },
        )
        .unwrap();
        let loaded: BalanceResponse = from_binary(&data).unwrap();
        assert_eq!(loaded.balance, amount1);

        // check balance query (empty)
        let data = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::Balance {
                address: HumanAddr::from("addr0002"),
            },
        )
        .unwrap();
        let loaded: BalanceResponse = from_binary(&data).unwrap();
        assert_eq!(loaded.balance, Uint128::zero());
    }

    #[test]
    fn transfer() {
        let mut deps = mock_dependencies(&coins(2, "token"));
        let addr1 = HumanAddr::from("addr0001");
        let addr2 = HumanAddr::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot transfer nothing
        let info = mock_info(addr1.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: too_much,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(
            err,
            ContractError::Std(StdError::Underflow { .. })
        ));

        // cannot send from empty account
        let info = mock_info(addr2.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr1.clone(),
            amount: transfer,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(
            err,
            ContractError::Std(StdError::Underflow { .. })
        ));

        // valid transfer
        let info = mock_info(addr1.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: transfer,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let remainder = (amount1 - transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), &addr1), remainder);
        assert_eq!(get_balance(deps.as_ref(), &addr2), transfer);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
    }

    #[test]
    fn burn() {
        let mut deps = mock_dependencies(&coins(2, "token"));
        let addr1 = HumanAddr::from("addr0001");
        let amount1 = Uint128::from(12340000u128);
        let burn = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot burn nothing
        let info = mock_info(addr1.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn {
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );

        // cannot burn more than we have
        let info = mock_info(addr1.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn { amount: too_much };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(
            err,
            ContractError::Std(StdError::Underflow { .. })
        ));
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );

        // valid burn reduces total supply
        let info = mock_info(addr1.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn { amount: burn };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let remainder = (amount1 - burn).unwrap();
        assert_eq!(get_balance(deps.as_ref(), &addr1), remainder);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            remainder
        );
    }

    #[test]
    fn send() {
        let mut deps = mock_dependencies(&coins(2, "token"));
        let addr1 = HumanAddr::from("addr0001");
        let contract = HumanAddr::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);
        let send_msg = Binary::from(r#"{"some":123}"#.as_bytes());

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot send nothing
        let info = mock_info(addr1.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: Uint128::zero(),
            msg: Some(send_msg.clone()),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: too_much,
            msg: Some(send_msg.clone()),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(
            err,
            ContractError::Std(StdError::Underflow { .. })
        ));

        // valid transfer
        let info = mock_info(addr1.clone(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: transfer,
            msg: Some(send_msg.clone()),
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        // ensure proper send message sent
        // this is the message we want delivered to the other side
        let binary_msg = Cw20ReceiveMsg {
            sender: addr1.clone(),
            amount: transfer,
            msg: Some(send_msg),
        }
        .into_binary()
        .unwrap();
        // and this is how it must be wrapped for the vm to process it
        assert_eq!(
            res.messages[0],
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract.clone(),
                msg: binary_msg,
                send: vec![],
            })
        );

        // ensure balance is properly transferred
        let remainder = (amount1 - transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), &addr1), remainder);
        assert_eq!(get_balance(deps.as_ref(), &contract), transfer);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
    }
}
