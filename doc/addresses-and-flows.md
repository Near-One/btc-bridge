# Bridge Addresses, Deposit & Withdraw

How the bridge controls BTC addresses without holding any private keys, and how the two
main flows work: **deposit goes directly through the btc-connector**, **withdraw goes
back through Omni Bridge**.

## How addresses work

The bridge holds **no private keys**. All BTC addresses are controlled by
[NEAR Chain Signatures (MPC)](https://docs.near.org/chain-abstraction/chain-signatures):
a public key is derived from the MPC root key, the bridge's NEAR account id, and a
derivation `path`. Only the bridge contract can request signatures for its own paths.

| Address | Derivation path | Purpose |
|---------|-----------------|---------|
| Deposit address | `sha256(DepositMsg)` | Unique per deposit message ([`get_user_deposit_address`](https://github.com/Near-One/btc-bridge/blob/e5666eaa16055cf484ab9a539ade0f454845f24c/contracts/satoshi-bridge/src/api/bridge.rs#L283)) |
| Change address | bridge account id | Pooled bridge funds (change outputs of withdrawals) |

Every UTXO stores the path it was received on. When the bridge spends UTXOs, the MPC
signs each input with that UTXO's own path — so deposited funds and pooled funds are
spendable only through the bridge contract.

## Deposit (BTC → nBTC)

Direct, no Omni Bridge: the user sends BTC to their deposit address, a relayer submits
a Merkle proof, the BTC Light Client verifies it, and the connector mints nBTC to
`recipient_id`. The deposit UTXO joins the bridge pool.

```mermaid
%%{init: {'theme': 'base', 'themeVariables': {'actorTextColor': '#000000', 'actorLineColor': '#333333', 'signalColor': '#333333', 'signalTextColor': '#000000', 'noteBkgColor': '#fff9c4', 'noteTextColor': '#000000', 'messageFontSize': '14px'}}}%%
sequenceDiagram
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
        participant B as btc_connector
        participant LC as BTC Light Client
        participant N as nBTC
    end

    U->>B: get_user_deposit_address(DepositMsg)
    B-->>U: BTC address (path = sha256(DepositMsg))
    U->>BTC: send BTC
    Note over BTC: transaction confirmed
    R->>B: verify_deposit_v2(deposit_msg,<br/>tx_bytes, vout, merkle proof)
    B->>LC: verify_transaction_inclusion(tx_id, merkle proof)
    LC-->>B: valid
    B->>N: mint nBTC to recipient_id
    Note over B: deposit UTXO joins the bridge pool
```

## Withdraw (nBTC → BTC)

Via Omni Bridge: the user sends nBTC to Omni Bridge with a `btc:` recipient. The Omni
relayer selects UTXOs from the bridge pool off-chain and calls
`submit_transfer_to_utxo_chain_connector` on Omni Bridge, which forwards the tokens to
the connector with a `Withdraw` message. The connector validates the transaction and
builds the PSBT; the relayer then requests an MPC signature for each input
(`sign_btc_transaction`), broadcasts the signed transaction, and after on-chain
confirmation the nBTC is burned.

```mermaid
%%{init: {'theme': 'base', 'themeVariables': {'actorTextColor': '#000000', 'actorLineColor': '#333333', 'signalColor': '#333333', 'signalTextColor': '#000000', 'noteBkgColor': '#fff9c4', 'noteTextColor': '#000000', 'messageFontSize': '14px'}}}%%
sequenceDiagram
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
        participant B as btc_connector
        participant LC as BTC Light Client
        participant MPC as Chain Signatures (MPC)
        participant N as nBTC
        participant O as Omni Bridge
    end

    U->>N: ft_transfer_call(omni.bridge.near,<br/>recipient: "btc:bc1q...")
    N->>O: nBTC locked on Omni
    Note over R: selects UTXOs from the bridge pool<br/>(input + output, off-chain)
    R->>O: submit_transfer_to_utxo_chain_connector
    O->>B: ft_transfer_call: nBTC + Withdraw msg
    Note over B: validate tx, reserve selected UTXOs,<br/>build PSBT
    R->>B: sign_btc_transaction (per input)
    B->>MPC: sign(payload, path per UTXO)
    MPC-->>B: signatures
    R->>BTC: broadcast signed tx
    BTC-->>U: BTC at target address
    R->>B: verify_withdraw_v2(tx_id, merkle proof)
    B->>LC: verify_transaction_inclusion(tx_id, merkle proof)
    LC-->>B: valid
    B->>N: burn nBTC
```

## References

- [NEAR Chain Signatures](https://docs.near.org/chain-abstraction/chain-signatures) — MPC key derivation and signing
- [Key Architecture](https://github.com/Near-One/btc-bridge/blob/omni-main/CLAUDE.md#key-architecture) — contracts and trust model
- [`get_user_deposit_address`](https://github.com/Near-One/btc-bridge/blob/e5666eaa16055cf484ab9a539ade0f454845f24c/contracts/satoshi-bridge/src/api/bridge.rs#L283) — deposit address derivation
