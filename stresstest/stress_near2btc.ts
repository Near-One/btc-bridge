// Stress test: NEAR -> BTC withdraw flow.
//
// Plan:
//   1. Print starting nBTC balance of the sender.
//   2. For i = 0..N-1:
//        a. GET gas_fee from bridge fee API for the planned transfer.
//        b. Spawn `bridge-cli testnet near-init-transfer --token near:nbtc.n-bridge.testnet
//           --amount 5000 --recipient btc:tb1q... --fee <gas_fee> --native-fee 0`.
//           bridge-cli reads its own NEAR signer credentials from env / its keystore;
//           this script never reads or prints them.
//        c. Capture stdout/stderr and append a record to results_near2btc.jsonl.

import { spawn } from "node:child_process";
import { appendFile } from "node:fs/promises";

const NEAR_NETWORK = "testnet";

const SENDER_NEAR_ACCOUNT = "olga24912_3.testnet";
const SENDER_OMNI = `near:${SENDER_NEAR_ACCOUNT}`;

// nBTC token (NEP-141) on NEAR testnet.
// https://testnet.nearblocks.io/address/nbtc.n-bridge.testnet
const NBTC_CONTRACT_ID = "nbtc.n-bridge.testnet";
const NBTC_OMNI = `near:${NBTC_CONTRACT_ID}`;
const NEAR_RPC_URL = "https://rpc.testnet.near.org";

// Withdraw target (BTC testnet address).
const BTC_RECIPIENT_ADDRESS = "tb1qdczmwzc75ef8dj66xcxqyuu2hmpzgj9uveuwgx";
const BTC_RECIPIENT_OMNI = `btc:${BTC_RECIPIENT_ADDRESS}`;

const WITHDRAW_AMOUNT_SATS = 5000;
const NATIVE_TOKEN_FEE = "0";

const FEE_API = "https://testnet.api.bridge.nearone.org/api/v2/transfer-fee";
const RESULTS_FILE = "results_near2btc.jsonl";

interface FeeQuote {
  native_token_fee: string;
  transferred_token_fee: string;
  gas_fee: string;
  protocol_fee: string;
  min_amount: string;
  usd_fee: number;
  insufficient_utxo: boolean;
}

interface WithdrawRecord {
  index: number;
  amount: number;
  recipient: string;
  gas_fee: string;
  protocol_fee: string;
  bridge_cli_output: string;
  exit_code: number;
}

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

async function getFeeQuote(amount: number): Promise<FeeQuote> {
  const url =
    `${FEE_API}?sender=${encodeURIComponent(SENDER_OMNI)}` +
    `&recipient=${encodeURIComponent(BTC_RECIPIENT_OMNI)}` +
    `&token=${encodeURIComponent(NBTC_OMNI)}` +
    `&amount=${amount}`;
  const r = await fetch(url);
  if (!r.ok) throw new Error(`fee api: ${r.status} ${await r.text()}`);
  return (await r.json()) as FeeQuote;
}

// Runs `bridge-cli testnet near-init-transfer ...` inheriting env (so its own NEAR
// signer/key env vars are available). Returns the captured combined output and exit code.
function runNearInitTransfer(
  amount: number,
  fee: string,
  nativeFee: string,
): Promise<{ output: string; code: number }> {
  return new Promise((resolve, reject) => {
    const args = [
      NEAR_NETWORK,
      "near-init-transfer",
      "--token",
      NBTC_CONTRACT_ID,
      "--amount",
      String(amount),
      "--recipient",
      BTC_RECIPIENT_OMNI,
      "--fee",
      fee,
      "--native-fee",
      nativeFee,
    ];
    const shellQuoted = args
      .map((a) => (/[\s'"]/.test(a) ? `'${a.replace(/'/g, "'\\''")}'` : a))
      .join(" ");
    console.log(`$ bridge-cli ${shellQuoted}`);
    const child = spawn("bridge-cli", args, { stdio: ["ignore", "pipe", "pipe"] });
    let out = "";
    child.stdout.on("data", (d) => (out += d.toString()));
    child.stderr.on("data", (d) => (out += d.toString()));
    child.on("error", reject);
    child.on("close", (code) => resolve({ output: out, code: code ?? -1 }));
  });
}

async function appendResult(rec: WithdrawRecord): Promise<void> {
  await appendFile(RESULTS_FILE, JSON.stringify(rec) + "\n");
}

async function main() {
  const balance = await ftBalanceOf(NBTC_CONTRACT_ID, SENDER_NEAR_ACCOUNT);
  console.log(`nBTC balance of ${SENDER_NEAR_ACCOUNT}: ${balance}`);

  const N = 5;
  for (let i = 0; i < N; i++) {
    const quote = await getFeeQuote(WITHDRAW_AMOUNT_SATS);
    console.log(
      `[${i}] quote: gas_fee=${quote.gas_fee} protocol_fee=${quote.protocol_fee} ` +
        `min_amount=${quote.min_amount} insufficient_utxo=${quote.insufficient_utxo}`,
    );
    if (quote.insufficient_utxo) {
      throw new Error("bridge reports insufficient_utxo — cannot proceed");
    }
    if (Number(quote.min_amount) > WITHDRAW_AMOUNT_SATS) {
      throw new Error(
        `WITHDRAW_AMOUNT_SATS=${WITHDRAW_AMOUNT_SATS} below min_amount=${quote.min_amount}`,
      );
    }

    const { output, code } = await runNearInitTransfer(
      WITHDRAW_AMOUNT_SATS,
      quote.gas_fee,
      NATIVE_TOKEN_FEE,
    );
    const cleaned = output.replace(/\x1b\[[0-9;]*m/g, "");
    console.log(`[${i}] bridge-cli exit=${code}\n${cleaned}`);

    await appendResult({
      index: i,
      amount: WITHDRAW_AMOUNT_SATS,
      recipient: BTC_RECIPIENT_OMNI,
      gas_fee: quote.gas_fee,
      protocol_fee: quote.protocol_fee,
      bridge_cli_output: cleaned,
      exit_code: code,
    });

    if (code !== 0) throw new Error(`bridge-cli exited ${code}`);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
