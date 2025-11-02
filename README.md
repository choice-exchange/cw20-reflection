# CW20-Reflection Spec: A CW20 reflection token implementation

CW20-Reflection is a specification for creating reflection tokens based on CosmWasm.
The name and design is based on the CW20 standard, with modifications made to allow the application of taxes, burns, and reflections.

The specification is split into multiple sections, a contract may only
implement some of this functionality, but must implement the base.

## Mainnet Deployment

Reflection Token Code Id: `1893`

Treasury Code Id: `1894`

## Note

As much as possible, the original CW20 standard has been left untouched. Instead, additional function signatures were added to allow for a "reflection" behavior.

## Design

The standard has been built in a way to utilize 2 contracts:

- Reflection treasury: Any reflection and taxes are processed in the treasury contract. The CW20 Taxed Token is the owner of the treasury. Developers are able to retrieve the reflected amounts out of the treasury, and separately airdrop the amounts to their users.
- CW20 Taxed Token: This contract is a modified version of the CW20 to allow tax-on-transfer to happen. All `ExecuteMsg` and `QueryMsg` are preserved. Additional function signatures have been added to cater for the taxation logic.


## Rules of engagement

Before we begin, it is important to understand the rules of engagement of the CW20-Reflection standard, so developers can plan around this to create unique mechanics:

- Amounts are taxed upon transfers. This means any usage of `transfer`, `transfer_from`, `send`, `send_from` messages will incur a tax on recipient amounts.
- When using `send` or `send_from`, the DEDUCTED AMOUNT is relayed via the Cw20ReceiveMsg. This means developers need not account for the deducted amount manually via their contracts.
- Whitelisted EOAs are exempt from taxes
- Anti-whale mechanism has been added to prevent over-transferring of too huge of a supply. This prevents wild fluctuations resulting from over auto-liquidity mechanisms

### Messages

`Transfer{recipient, amount}` - Moves `amount` CW20 tokens from the `info.sender` account to the `recipient` account. This is designed to send to an address controlled by a private key and does not trigger any actions on the recipient if it is a contract.

`Send{contract, amount, msg}` - Moves `amount` CW20 tokens from the `info.sender` account to the `contract` account. `contract` must be an address of a contract that implements the `Receiver` interface. The msg will be passed to the recipient contract, along with the amount.

Of course. Based on your successful debugging journey and the final, correct understanding of the system's architecture, here is a clear, updated, and comprehensive README. It includes a step-by-step deployment guide that will ensure anyone using your contract can set it up correctly.

## Deployment and Configuration Guide

Follow these steps precisely to ensure a successful and functional deployment.

### Step 1: Instantiate the `REFLECT` Token Contract

First, you must deploy the main token contract. This transaction will also automatically deploy the Treasury contract for you.

**`InstantiateMsg` Example:**
```json
{
  "name": "My Reflection Token",
  "symbol": "REFLECT",
  "decimals": 6,
  "initial_balances": [
    {
      "address": "inj1your_wallet_address...",
      "amount": "1000000000000"
    }
  ],
  "mint": null,
  "marketing": null,
  "admin": "inj1your_admin_address...",
  "router": "inj1the_dex_router_address...",
  "cw20_code_id": 1234
}
```

*   **`admin`**: The address with permission to change tax rates and whitelist addresses.
*   **`router`**: The address of the DEX's router contract, which the Treasury will use for swaps.
*   **`cw20_code_id`**: The code ID of your compiled **Treasury** contract wasm.

After this transaction succeeds, query the contract state to find the address of your newly deployed **Treasury** contract.

### Step 2: Set the Global Tax Rate

Now, configure the desired tax settings. This is a single transaction sent to the `REFLECT` token contract.

**`ExecuteMsg` Example:**
```json
{
  "set_tax_rate": {
    "global_rate": "0.05",
    "reflection_rate": "1.0",
    "burn_rate": "0.0",
    "antiwhale_rate": "0.02"
  }
}
```

*   **`global_rate`**: The total tax percentage (e.g., "0.05" is 5%).
*   **`reflection_rate`**: The portion of the tax to be used for reflection/swapping in the treasury (e.g., "1.0" is 100%).
*   **`burn_rate`**: The portion of the tax to be burned.
*   **`antiwhale_rate`**: The percentage of total supply that triggers the anti-whale mechanism (e.g., "0.02" is 2%).

### Step 3: Whitelist Critical Infrastructure

To prevent the DEX's internal accounting from breaking, you must whitelist all contracts that handle fees and liquidity. This is a series of `ExecuteMsg` calls to the `REFLECT` token contract.

**Whitelist the DEX's CW20 Adapter / Fee Collector:** This is the contract that handles fees. 
```json
{
  "set_whitelist": {
    "user": "inj1the_dex_cw20_adapter_address...",
    "enable": true
  }
}
```

**Whitelist the DEX's Factory:** This is the contract that will perform liquidity provision if you add liquidity when creating the pair.
```json
{
  "set_whitelist": {
    "user": "inj1the_dex_factory_address...",
    "enable": true
  }
}
```

### Step 4: Configure the Treasury Pairs

Finally, you must tell the Treasury which liquidity pools to use for its operations. This is a series of `ExecuteMsg` calls sent to your **Treasury contract address**.

**Set the Liquidity Pair:** This defines the primary `REFLECT`/`INJ` pool. The Treasury uses this for adding liquidity (if `reflection_rate` is less than 1.0).
```json
{
  "set_liquidity_pair": {
    "asset_infos": [
      {
        "token": {
          "contract_addr": "inj1your_reflect_token_address..."
        }
      },
      {
        "native_token": {
          "denom": "inj"
        }
      }
    ],
    "pair_contract": "inj1the_dex_pair_address..."
  }
}
```

**Set the Reflection Pair:** This defines the swap route for converting the collected tax. If your goal is to collect `INJ`, this will be the same as the liquidity pair.
```json
{
  "set_reflection_pair": {
    "asset_infos": [
      {
        "token": {
          "contract_addr": "inj1your_reflect_token_address..."
        }
      },
      {
        "native_token": {
          "denom": "inj"
        }
      }
    ],
    "pair_contract": "inj1the_dex_pair_address..."
  }
}
```
