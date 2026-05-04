// Stress test for NEAR <-> BTC bridge.
//
// Plan:
//   1. For nonces 0, 1, 2, ..., N-1 call `bridge-cli` with --fee=<nonce> to get
//      a unique BTC deposit address for each.
//   2. Save mapping (fee -> address) to results file.
//   3. Send BTC to each deposit address.
//   4. (Later) trigger verify_deposit and measure throughput.

import { spawn } from "node:child_process";
import { appendFile } from "node:fs/promises";
import * as bitcoin from "bitcoinjs-lib";
import { ECPairFactory } from "ecpair";
import * as ecc from "tiny-secp256k1";

const ECPair = ECPairFactory(ecc);
bitcoin.initEccLib(ecc);

// Bridge contract (resolved by bridge-cli from network name):
// https://testnet.nearblocks.io/address/btc-connector.n-bridge.testnet
const NEAR_NETWORK = "testnet";

const FINAL_RECIPIENT = "near:olga24912_3.testnet";
const NEAR_RECIPIENT_ACCOUNT = "olga24912_3.testnet";
const CHAIN = "btc";

// https://testnet.nearblocks.io/address/nbtc.n-bridge.testnet
const NBTC_CONTRACT_ID = "nbtc.n-bridge.testnet";
const NEAR_RPC_URL = "https://rpc.testnet.near.org";

const RESULTS_FILE = "results.jsonl";
const MEMPOOL_API = "https://mempool.space/testnet/api";
const BTC_NETWORK = bitcoin.networks.testnet;

const DEPOSIT_AMOUNT_SATS = 8000;
const FEE_RATE_SAT_PER_VB = 2;

interface DepositRecord {
  fee: number;
  deposit_address: string;
  txid: string;
}

interface Utxo {
  txid: string;
  vout: number;
  value: number;
}

// Runs `bridge-cli testnet get-bitcoin-address --chain btc --recipient-id <r> --fee <fee>`
// and parses the BTC address out of the stderr/stdout log line:
//   "<timestamp> BTC Address: tb1q..."
function getBitcoinAddress(fee: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const args = [
      NEAR_NETWORK,
      "get-bitcoin-address",
      "--chain",
      CHAIN,
      "--recipient-id",
      FINAL_RECIPIENT,
      "--fee",
      String(fee),
    ];
    const child = spawn("bridge-cli", args);
    let out = "";
    child.stdout.on("data", (d) => (out += d.toString()));
    child.stderr.on("data", (d) => (out += d.toString()));
    child.on("error", reject);
    child.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`bridge-cli exited ${code}: ${out}`));
        return;
      }
      const clean = out.replace(/\x1b\[[0-9;]*m/g, "");
      const m = clean.match(/BTC Address:\s*([a-zA-Z0-9]+)/);
      if (!m) {
        reject(new Error(`could not parse BTC address from output: ${out}`));
        return;
      }
      resolve(m[1]);
    });
  });
}

// Loads the source-wallet keypair from BTC_TESTNET_WIF env var.
// The WIF is never logged.
function loadKeyPair() {
  const wif = process.env.BTC_TESTNET_WIF;
  if (!wif) {
    throw new Error("BTC_TESTNET_WIF env var is required (testnet WIF private key)");
  }
  return ECPair.fromWIF(wif, BTC_NETWORK);
}

// Derives the P2WPKH (bech32, tb1q...) address for a keypair.
function p2wpkhAddress(keyPair: ReturnType<typeof loadKeyPair>): string {
  const { address } = bitcoin.payments.p2wpkh({
    pubkey: Buffer.from(keyPair.publicKey),
    network: BTC_NETWORK,
  });
  if (!address) throw new Error("failed to derive P2WPKH address");
  return address;
}

async function fetchUtxos(address: string): Promise<Utxo[]> {
  const r = await fetch(`${MEMPOOL_API}/address/${address}/utxo`);
  if (!r.ok) throw new Error(`mempool utxo: ${r.status} ${await r.text()}`);
  const raw = (await r.json()) as { txid: string; vout: number; value: number }[];
  return raw.map((u) => ({ txid: u.txid, vout: u.vout, value: u.value }));
}

async function broadcastTx(hex: string): Promise<string> {
  const r = await fetch(`${MEMPOOL_API}/tx`, {
    method: "POST",
    headers: { "Content-Type": "text/plain" },
    body: hex,
  });
  const text = await r.text();
  if (!r.ok) throw new Error(`broadcast failed: ${r.status} ${text}`);
  return text.trim();
}

// Sends `amountSats` to `toAddress` from the loaded keypair using a single
// P2WPKH input. Returns the broadcast txid.
async function sendBtc(toAddress: string, amountSats: number): Promise<string> {
  const keyPair = loadKeyPair();
  const fromAddress = p2wpkhAddress(keyPair);
  const utxos = await fetchUtxos(fromAddress);
  if (utxos.length === 0) throw new Error(`no UTXOs on ${fromAddress}`);

  // Estimate fee: 1 input + 2 outputs P2WPKH ~= 141 vbytes.
  const estVbytes = 141;
  const feeSats = estVbytes * FEE_RATE_SAT_PER_VB;
  const need = amountSats + feeSats;

  // Pick the smallest UTXO that covers the need (reduces dust accumulation).
  const sorted = [...utxos].sort((a, b) => a.value - b.value);
  const utxo = sorted.find((u) => u.value >= need);
  if (!utxo) {
    throw new Error(
      `no single UTXO covers ${need} sats; largest is ${sorted[sorted.length - 1].value}`,
    );
  }

  const psbt = new bitcoin.Psbt({ network: BTC_NETWORK });
  const witnessScript = bitcoin.payments.p2wpkh({
    pubkey: Buffer.from(keyPair.publicKey),
    network: BTC_NETWORK,
  }).output!;
  psbt.addInput({
    hash: utxo.txid,
    index: utxo.vout,
    witnessUtxo: { script: witnessScript, value: utxo.value },
  });
  psbt.addOutput({ address: toAddress, value: amountSats });
  const change = utxo.value - amountSats - feeSats;
  if (change > 546) {
    psbt.addOutput({ address: fromAddress, value: change });
  }
  psbt.signInput(0, {
    publicKey: Buffer.from(keyPair.publicKey),
    sign: (hash: Buffer) => Buffer.from(keyPair.sign(hash)),
  });
  psbt.finalizeAllInputs();
  const tx = psbt.extractTransaction();
  return broadcastTx(tx.toHex());
}

async function appendResult(rec: DepositRecord): Promise<void> {
  await appendFile(RESULTS_FILE, JSON.stringify(rec) + "\n");
}

// View-call ft_balance_of on a NEP-141 token contract via NEAR JSON-RPC.
async function ftBalanceOf(token: string, accountId: string): Promise<string> {
  const args = Buffer.from(JSON.stringify({ account_id: accountId })).toString("base64");
  const r = await fetch(NEAR_RPC_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "query",
      params: {
        request_type: "call_function",
        finality: "final",
        account_id: token,
        method_name: "ft_balance_of",
        args_base64: args,
      },
    }),
  });
  const j = (await r.json()) as {
    result?: { result: number[] };
    error?: { message: string };
  };
  if (j.error) throw new Error(`ft_balance_of: ${j.error.message}`);
  if (!j.result) throw new Error(`ft_balance_of: empty response`);
  return JSON.parse(Buffer.from(j.result.result).toString());
}

async function main() {
  const balance = await ftBalanceOf(NBTC_CONTRACT_ID, NEAR_RECIPIENT_ACCOUNT);
  console.log(`nBTC balance of ${NEAR_RECIPIENT_ACCOUNT}: ${balance}`);

  const N = 1;
  for (let fee = 0; fee < N; fee++) {
    const depositAddress = await getBitcoinAddress(fee);
    console.log(`fee=${fee} -> ${depositAddress}`);
    const txid = await sendBtc(depositAddress, DEPOSIT_AMOUNT_SATS);
    console.log(`  sent ${DEPOSIT_AMOUNT_SATS} sats, txid=${txid}`);
    await appendResult({ fee, deposit_address: depositAddress, txid });
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
