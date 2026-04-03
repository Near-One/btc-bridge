# Refund Reject → Deposit succeeds

User requests refund, DAO rejects it. After rejection the UTXO is NOT marked
in `verified_deposit_utxo`, so normal `verify_deposit` works and nBTC is minted.

## File References

| # | Method | File |
|---|--------|------|
| 1 | `get_user_deposit_address` | `contracts/satoshi-bridge/src/api/bridge.rs:406` |
| 2 | `request_refund` | `contracts/satoshi-bridge/src/api/bridge.rs:426` |
| 3 | `verify_transaction_inclusion` | `contracts/satoshi-bridge/src/btc_light_client/mod.rs:113` |
| 4 | `request_refund_callback` | `contracts/satoshi-bridge/src/refund.rs` |
| 5 | `reject_refund` | `contracts/satoshi-bridge/src/api/bridge.rs:447` |
| 6 | `verify_deposit` | `contracts/satoshi-bridge/src/api/bridge.rs:22` |
| 7 | `verify_deposit_callback` | `contracts/satoshi-bridge/src/btc_light_client/deposit.rs:147` |
| 8 | `mint` | `contracts/nbtc/src/lib.rs:118` |
| 9 | `mint_callback` | `contracts/satoshi-bridge/src/nbtc/mint.rs:46` |

## Sequence Diagram

```mermaid
%%{init: {'theme': 'base', 'themeVariables': {'actorTextColor': '#000000', 'actorLineColor': '#333333', 'signalColor': '#333333', 'signalTextColor': '#000000', 'noteBkgColor': '#fff9c4', 'noteTextColor': '#000000', 'messageFontSize': '14px'}}}%%
sequenceDiagram
    autonumber

    box rgb(224, 224, 224) Off-chain
        participant U as User
    end

    box rgb(255, 224, 178) Bitcoin
        participant BTC as Bitcoin Network
    end

    box rgb(224, 224, 224) Off-chain
        participant R as Relayer
    end

    box rgb(187, 222, 251) NEAR Protocol
        participant B as btc_connector<br/>(satoshi-bridge)
        participant LC as BTC Light Client
        participant N as nBTC Contract
    end

    U->>B: get_user_deposit_address(DepositMsg {<br/>recipient_id, refund_address: "bc1q..."})<br/>📄 api/bridge.rs:406
    B-->>U: BTC deposit address

    U->>BTC: Send BTC to deposit address
    Note over BTC: Transaction confirmed

    U->>B: request_refund(deposit_msg, tx_proof)<br/>📄 api/bridge.rs:426

    B->>LC: verify_transaction_inclusion(tx_id, merkle_proof)<br/>📄 btc_light_client/mod.rs:113
    LC-->>B: valid

    Note over B: request_refund_callback
    B->>B: Save RefundRequest

    R->>B: reject_refund(utxo_storage_key)<br/>📄 api/bridge.rs:447
    Note over B: RefundRequest removed,<br/>UTXO NOT marked in verified_deposit_utxo

    rect rgb(255, 200, 200)
        U->>B: execute_refund(utxo_storage_key)<br/>📄 api/bridge.rs:454
        Note over B: PANIC: "Refund request not found"
    end

    Note over R,B: Normal deposit flow proceeds

    R->>B: verify_deposit(deposit_msg, tx_proof)<br/>📄 api/bridge.rs:22

    B->>LC: verify_transaction_inclusion(tx_id, merkle_proof)<br/>📄 btc_light_client/mod.rs:113
    LC-->>B: valid

    Note over B: verify_deposit_callback<br/>📄 btc_light_client/deposit.rs:147

    B->>N: mint(recipient_id, mint_amount)<br/>📄 nbtc/src/lib.rs:118
    N-->>B: OK

    Note over B: mint_callback<br/>📄 nbtc/mint.rs:46
    B-->>U: nBTC credited to recipient
```
