# Refund Flow (BTC → BTC, deposit never finalized)

User deposited BTC with `refund_address` in DepositMsg, but `verify_deposit` was never called.
Anyone who knows the deposit_msg can request a refund. After timelock passes, DAO/Operator
executes the refund — BTC is sent back to the refund address.

## File References

| # | Method | File |
|---|--------|------|
| 1 | `get_user_deposit_address` | `contracts/satoshi-bridge/src/api/bridge.rs:406` |
| 2 | `DepositMsg.refund_address` | `contracts/satoshi-bridge/src/deposit_msg.rs:27` |
| 3 | `request_refund` | `contracts/satoshi-bridge/src/api/bridge.rs:426` |
| 4 | `verify_transaction_inclusion` | `contracts/satoshi-bridge/src/btc_light_client/mod.rs:113` |
| 5 | `request_refund_callback` | `contracts/satoshi-bridge/src/refund.rs:170` |
| 6 | `execute_refund` | `contracts/satoshi-bridge/src/api/bridge.rs:454` |
| 7 | `sign_btc_transaction` | `contracts/satoshi-bridge/src/api/chain_signatures.rs:21` |
| 8 | `sign` (MPC) | `contracts/satoshi-bridge/src/chain_signature.rs:57` |
| 9 | `sign_btc_transaction_callback` | `contracts/satoshi-bridge/src/chain_signature.rs:135` |
| 10 | `verify_refund` | `contracts/satoshi-bridge/src/api/bridge.rs` |
| 11 | `verify_refund_callback` | `contracts/satoshi-bridge/src/refund.rs` |

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
        participant MPC as Chain Signatures<br/>(MPC)
    end

    U->>B: get_user_deposit_address(DepositMsg {<br/>recipient_id, refund_address: "bc1q..."})<br/>📄 api/bridge.rs:406
    B-->>U: BTC deposit address

    U->>BTC: Send BTC to deposit address
    Note over BTC: Transaction confirmed
    Note over B: verify_deposit never called<br/>(relayer down, user changed mind, etc.)

    U->>B: request_refund(deposit_msg,<br/>tx_bytes, vout, tx_block_blockhash,<br/>tx_index, merkle_proof)<br/>📄 api/bridge.rs:426

    B->>LC: verify_transaction_inclusion(tx_id, merkle_proof)<br/>📄 btc_light_client/mod.rs:113
    LC-->>B: valid / invalid

    Note over B: request_refund_callback<br/>📄 refund.rs:170
    B->>B: Save RefundRequest {<br/>utxo_storage_key, amount, created_at}

    rect rgb(255, 200, 200)
        U->>B: execute_refund(utxo_storage_key)<br/>📄 api/bridge.rs:454
        Note over B: PANIC: "Refund timelock has not passed yet"
    end

    Note over B: Timelock period passes<br/>(refund_timelock_sec)

    U->>B: execute_refund(utxo_storage_key)<br/>📄 api/bridge.rs:454
    Note over B: Check: timelock passed?<br/>Check: UTXO not in verified_deposit_utxo?
    Note over B: Build PSBT:<br/>input = deposit UTXO<br/>output = refund_address<br/>remainder = gas fee

    U->>B: sign_btc_transaction(sign_index)<br/>📄 api/chain_signatures.rs:21

    B->>MPC: sign(payload, path, key_version)<br/>📄 chain_signature.rs:57
    MPC-->>B: signature

    Note over B: sign_btc_transaction_callback<br/>📄 chain_signature.rs:135

    U->>BTC: Broadcast refund transaction
    BTC-->>U: BTC returned to refund_address

    R->>B: verify_refund(tx_id, tx_block_blockhash,<br/>tx_index, merkle_proof)<br/>📄 api/bridge.rs

    B->>LC: verify_transaction_inclusion(tx_id, merkle_proof)<br/>📄 btc_light_client/mod.rs:113
    LC-->>B: valid

    Note over B: verify_refund_callback<br/>📄 refund.rs
    B->>B: Remove BTCPendingInfo
```
