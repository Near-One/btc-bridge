# Refund — second request_refund after execute blocked

User requests refund, executes it. Then tries `request_refund` again
for the same UTXO — fails because `execute_refund` marked the UTXO
in `verified_deposit_utxo`.

## File References

| # | Method | File |
|---|--------|------|
| 1 | `get_user_deposit_address` | `contracts/satoshi-bridge/src/api/bridge.rs:406` |
| 2 | `request_refund` | `contracts/satoshi-bridge/src/api/bridge.rs:426` |
| 3 | `verify_transaction_inclusion` | `contracts/satoshi-bridge/src/btc_light_client/mod.rs:113` |
| 4 | `request_refund_callback` | `contracts/satoshi-bridge/src/refund.rs` |
| 5 | `execute_refund` | `contracts/satoshi-bridge/src/api/bridge.rs:454` |

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
    end

    U->>B: get_user_deposit_address(DepositMsg {<br/>recipient_id, refund_address: "bc1q..."})<br/>📄 api/bridge.rs:406
    B-->>U: BTC deposit address

    U->>BTC: Send BTC to deposit address
    Note over BTC: Transaction confirmed

    U->>B: request_refund(deposit_msg, tx_proof)<br/>📄 api/bridge.rs:426

    B->>LC: verify_transaction_inclusion(tx_id, merkle_proof)
    LC-->>B: valid

    Note over B: request_refund_callback
    B->>B: Save RefundRequest

    Note over B: Timelock passes

    U->>B: execute_refund(utxo_storage_key)<br/>📄 api/bridge.rs:454
    Note over B: UTXO added to verified_deposit_utxo

    Note over U,B: User tries request_refund again

    rect rgb(255, 200, 200)
        U->>B: request_refund(deposit_msg, tx_proof)<br/>📄 api/bridge.rs:426
        Note over B: PANIC: "UTXO already verified via deposit"
    end
```
