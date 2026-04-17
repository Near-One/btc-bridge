# Refund Race — safe_verify_deposit wins before execute_refund

User requests refund, but during timelock period the Relayer calls `safe_verify_deposit`
(OmniBridge integration). Safe deposit succeeds, UTXO is marked in `verified_deposit_utxo`.
When user tries `execute_refund` — it fails because UTXO is already finalized.

## File References

| # | Method | File |
|---|--------|------|
| 1 | `get_user_deposit_address` | `contracts/satoshi-bridge/src/api/bridge.rs:406` |
| 2 | `request_refund` | `contracts/satoshi-bridge/src/api/bridge.rs:426` |
| 3 | `verify_transaction_inclusion` | `contracts/satoshi-bridge/src/btc_light_client/mod.rs:113` |
| 4 | `request_refund_callback` | `contracts/satoshi-bridge/src/refund.rs` |
| 5 | `safe_verify_deposit` | `contracts/satoshi-bridge/src/api/bridge.rs:95` |
| 6 | `verify_safe_deposit_callback` | `contracts/satoshi-bridge/src/btc_light_client/deposit.rs:179` |
| 7 | `safe_mint` | `contracts/nbtc/src/lib.rs:96` |
| 8 | `safe_mint_callback` | `contracts/satoshi-bridge/src/btc_light_client/deposit.rs:214` |
| 9 | `execute_refund` | `contracts/satoshi-bridge/src/api/bridge.rs:454` |

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

    U->>B: get_user_deposit_address(DepositMsg {<br/>recipient_id, safe_deposit: {msg},<br/>refund_address: "bc1q..."})<br/>📄 api/bridge.rs:406
    B-->>U: BTC deposit address

    U->>BTC: Send BTC to deposit address
    Note over BTC: Transaction confirmed

    U->>B: request_refund(deposit_msg, tx_proof)<br/>📄 api/bridge.rs:426

    B->>LC: verify_transaction_inclusion(tx_id, merkle_proof)
    LC-->>B: valid

    Note over B: request_refund_callback
    B->>B: Save RefundRequest

    Note over B: Timelock period (waiting...)

    Note over R,B: Relayer calls safe_verify_deposit during timelock

    R->>B: safe_verify_deposit(deposit_msg, tx_proof)<br/>📄 api/bridge.rs:95

    B->>LC: verify_transaction_inclusion(tx_id, merkle_proof)
    LC-->>B: valid

    Note over B: verify_safe_deposit_callback

    B->>N: safe_mint(recipient, amount, msg)<br/>📄 nbtc/src/lib.rs:96
    N-->>B: OK

    Note over B: safe_mint_callback
    B-->>U: nBTC credited to recipient

    Note over U,B: User tries execute_refund after timelock

    rect rgb(255, 200, 200)
        U->>B: execute_refund(utxo_storage_key)<br/>📄 api/bridge.rs:454
        Note over B: PANIC: "UTXO already verified via deposit,<br/>cannot refund"
    end
```
