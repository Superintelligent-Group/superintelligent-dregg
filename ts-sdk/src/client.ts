import type {
  CellId,
  CellState,
  CreateCellParams,
  Turn,
  TurnReceipt,
  Block,
  BlockId,
  StarkProof,
  BearerCapProof,
  DelegationProofData,
  Intent,
  MatchSpec,
  Fulfillment,
  Artwork,
  SwapParams,
  SwapResult,
  StealthMetaAddress,
  PrivateTransferParams,
  PrivateTransferResult,
  NodeConfig,
} from "./types.js";

// ---------------------------------------------------------------------------
// Client Interface
// ---------------------------------------------------------------------------

/** The full Pyana client interface for interacting with a node. */
export interface PyanaClient {
  // -- Cells --
  /** Create a new cell (optionally from a factory). */
  createCell(params: CreateCellParams): Promise<CellId>;
  /** Get the current state of a cell. */
  getCell(id: CellId): Promise<CellState>;
  /** Transition a cell to sovereign mode. */
  makeSovereign(id: CellId): Promise<{ cellId: string; stateCommitment: string; mode: string }>;
  /** Verify the factory provenance of a cell. */
  verifyProvenance(cellVkHex: string, knownFactoryVks: string[]): Promise<{ fromFactory: boolean; factoryVk: string | null }>;

  // -- Turns --
  /** Submit a signed turn to the network. */
  submitTurn(turn: Turn): Promise<TurnReceipt>;
  /** Query the account balance. */
  queryBalance(): Promise<{ balance: number }>;

  // -- Blocks --
  /** Get a block by ID. */
  getBlock(id: BlockId): Promise<Block>;
  /** Get the latest block. */
  getLatestBlock(): Promise<Block>;

  // -- Gallery --
  /** List all artworks in the gallery. */
  listArtworks(): Promise<Artwork[]>;
  /** Place a bid on an auction (with commitment). */
  placeBid(auctionId: string, commitment: string): Promise<void>;

  // -- AMM --
  /** Execute a swap on a liquidity pool. */
  swap(poolId: string, params: SwapParams): Promise<SwapResult>;

  // -- Bearer Capabilities --
  /** Create a bearer capability token. */
  createBearerCap(targetCell: CellId, action: string, expiry?: number): Promise<DelegationProofData>;
  /** Verify a bearer capability token. */
  verifyBearerCap(proof: BearerCapProof): Promise<{ valid: boolean; expired: boolean }>;

  // -- Proofs --
  /** Compose multiple proofs into an aggregate. */
  composeProofs(proofs: StarkProof[], mode: "and" | "or" | "chain" | "aggregate"): Promise<{ composedProof: string; mode: string; inputCount: number; valid: boolean }>;

  // -- Intents --
  /** Post an intent to the network. */
  postIntent(matchSpec: MatchSpec, options?: { expiry?: number }): Promise<{ intentId: string; expiry: number }>;
  /** List active intents. */
  listIntents(filter?: { kind?: string }): Promise<Intent[]>;
  /** Fulfill an intent. */
  fulfillIntent(intentId: string, tokenId?: string): Promise<Fulfillment>;

  // -- Privacy --
  /** Get or derive a stealth meta-address. */
  getStealthAddress(): Promise<StealthMetaAddress>;
  /** Send a private transfer (amount hidden via Pedersen commitment). */
  privateTransfer(params: PrivateTransferParams): Promise<PrivateTransferResult>;
  /** Post an encrypted intent with SSE tokens. */
  postEncryptedIntent(matchSpec: MatchSpec, options?: { expiry?: number; keywords?: string[]; recipientPubkey?: number[] }): Promise<{ intentId: string; expiry: number; encrypted: boolean }>;

  // -- Peer Exchange --
  /** Initiate a peer exchange between sovereign cells. */
  peerExchange(receiverCellHex: string, amount: number): Promise<{ exchangeId: string; proofCommitment: string }>;

  // -- Node --
  /** Get the node status. */
  getStatus(): Promise<{ merkleRoot: string; height: number; publicKey?: string }>;
}

// ---------------------------------------------------------------------------
// HTTP Client Implementation
// ---------------------------------------------------------------------------

interface RequestOptions {
  method?: string;
  body?: unknown;
  headers?: Record<string, string>;
}

interface ApiResponse<T = unknown> {
  ok: boolean;
  data?: T;
  status?: number;
  error?: string;
}

async function request<T>(baseUrl: string, path: string, apiKey: string | undefined, options: RequestOptions = {}): Promise<ApiResponse<T>> {
  const url = baseUrl.replace(/\/$/, "") + path;
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(apiKey ? { "X-Devnet-Key": apiKey } : {}),
    ...(options.headers || {}),
  };

  const fetchOptions: RequestInit = {
    method: options.method || "GET",
    headers,
  };

  if (options.body !== undefined) {
    fetchOptions.body = JSON.stringify(options.body);
  }

  const resp = await fetch(url, fetchOptions);

  if (resp.ok) {
    const data = (await resp.json().catch(() => null)) as T | null;
    return { ok: true, data: data ?? undefined, status: resp.status };
  }

  const errText = await resp.text().catch(() => "");
  return { ok: false, error: `HTTP ${resp.status}: ${errText}`, status: resp.status };
}

function unwrap<T>(result: ApiResponse<T>, context: string): T {
  if (!result.ok || result.data === undefined) {
    throw new Error(`${context}: ${result.error || "unknown error"}`);
  }
  return result.data;
}

// ---------------------------------------------------------------------------
// createClient factory
// ---------------------------------------------------------------------------

/**
 * Create a Pyana client connected to a node.
 *
 * @param nodeUrl - Base URL of the Pyana node (e.g. "https://devnet.pyana.fg-goose.online")
 * @param apiKey - Optional devnet API key (sent as X-Devnet-Key header)
 */
export function createClient(nodeUrl: string, apiKey?: string): PyanaClient {
  const base = nodeUrl;

  return {
    // -- Cells --
    async createCell(params) {
      const result = await request<{ cell_id: string }>(base, "/cells", apiKey, {
        method: "POST",
        body: {
          owner_pubkey: params.ownerPubkeyHex,
          initial_balance: params.initialBalance ?? 0,
          factory_vk: params.factoryVkHex ?? null,
        },
      });
      return unwrap(result, "createCell").cell_id;
    },

    async getCell(id) {
      const result = await request<CellState>(base, `/cells/${id}`, apiKey);
      return unwrap(result, "getCell");
    },

    async makeSovereign(id) {
      const result = await request<{ cellId: string; stateCommitment: string; mode: string }>(
        base, `/cells/${id}/sovereign`, apiKey, { method: "POST" }
      );
      return unwrap(result, "makeSovereign");
    },

    async verifyProvenance(cellVkHex, knownFactoryVks) {
      const result = await request<{ fromFactory: boolean; factoryVk: string | null }>(
        base, "/cells/verify-provenance", apiKey, {
          method: "POST",
          body: { cell_vk: cellVkHex, known_factory_vks: knownFactoryVks },
        }
      );
      return unwrap(result, "verifyProvenance");
    },

    // -- Turns --
    async submitTurn(turn) {
      const result = await request<TurnReceipt>(base, "/turns/submit", apiKey, {
        method: "POST",
        body: turn,
      });
      return unwrap(result, "submitTurn");
    },

    async queryBalance() {
      const result = await request<{ balance: number }>(base, "/accounts/balance", apiKey);
      return unwrap(result, "queryBalance");
    },

    // -- Blocks --
    async getBlock(id) {
      const result = await request<Block>(base, `/blocks/${id}`, apiKey);
      return unwrap(result, "getBlock");
    },

    async getLatestBlock() {
      const result = await request<Block>(base, "/blocks/latest", apiKey);
      return unwrap(result, "getLatestBlock");
    },

    // -- Gallery --
    async listArtworks() {
      const result = await request<Artwork[]>(base, "/gallery/artworks", apiKey);
      return unwrap(result, "listArtworks");
    },

    async placeBid(auctionId, commitment) {
      const result = await request(base, `/gallery/auctions/${auctionId}/bid`, apiKey, {
        method: "POST",
        body: { commitment },
      });
      if (!result.ok) throw new Error(`placeBid: ${result.error}`);
    },

    // -- AMM --
    async swap(poolId, params) {
      const result = await request<SwapResult>(base, `/amm/pools/${poolId}/swap`, apiKey, {
        method: "POST",
        body: {
          token_in: params.tokenIn,
          token_out: params.tokenOut,
          amount_in: params.amountIn,
          slippage_bps: params.slippageBps ?? 50,
        },
      });
      return unwrap(result, "swap");
    },

    // -- Bearer Capabilities --
    async createBearerCap(targetCell, action, expiry) {
      const result = await request<DelegationProofData>(base, "/bearer-caps/create", apiKey, {
        method: "POST",
        body: { target_cell: targetCell, action, expiry: expiry ?? 0 },
      });
      return unwrap(result, "createBearerCap");
    },

    async verifyBearerCap(proof) {
      const result = await request<{ valid: boolean; expired: boolean }>(
        base, "/bearer-caps/verify", apiKey, {
          method: "POST",
          body: {
            bearer_token: proof.bearerTokenHex,
            delegator_key: proof.delegatorKeyHex,
            target_cell: proof.targetCell,
            action: proof.action,
            expiry: proof.expiry,
          },
        }
      );
      return unwrap(result, "verifyBearerCap");
    },

    // -- Proofs --
    async composeProofs(proofs, mode) {
      const result = await request<{ composedProof: string; mode: string; inputCount: number; valid: boolean }>(
        base, "/proofs/compose", apiKey, {
          method: "POST",
          body: {
            proofs: proofs.map((p) => ({
              proof_json: p.proofJson,
              public_inputs: p.publicInputs ?? [],
            })),
            mode,
          },
        }
      );
      return unwrap(result, "composeProofs");
    },

    // -- Intents --
    async postIntent(matchSpec, options) {
      const result = await request<{ intentId: string; expiry: number }>(base, "/intents", apiKey, {
        method: "POST",
        body: { match_spec: matchSpec, expiry: options?.expiry },
      });
      return unwrap(result, "postIntent");
    },

    async listIntents(filter) {
      const query = filter?.kind ? `?kind=${filter.kind}` : "";
      const result = await request<Intent[]>(base, `/intents${query}`, apiKey);
      return unwrap(result, "listIntents");
    },

    async fulfillIntent(intentId, tokenId) {
      const result = await request<Fulfillment>(base, `/intents/${intentId}/fulfill`, apiKey, {
        method: "POST",
        body: { token_id: tokenId ?? null },
      });
      return unwrap(result, "fulfillIntent");
    },

    // -- Privacy --
    async getStealthAddress() {
      const result = await request<StealthMetaAddress>(base, "/stealth/meta-address", apiKey);
      return unwrap(result, "getStealthAddress");
    },

    async privateTransfer(params) {
      const result = await request<PrivateTransferResult>(base, "/transfers/private", apiKey, {
        method: "POST",
        body: {
          amount: params.amount,
          asset_type: params.assetType,
          recipient_stealth_meta: params.recipientStealthMeta,
        },
      });
      return unwrap(result, "privateTransfer");
    },

    async postEncryptedIntent(matchSpec, options) {
      const result = await request<{ intentId: string; expiry: number; encrypted: boolean }>(
        base, "/intents/encrypted", apiKey, {
          method: "POST",
          body: {
            match_spec: matchSpec,
            expiry: options?.expiry,
            keywords: options?.keywords,
            recipient_pubkey: options?.recipientPubkey,
          },
        }
      );
      return unwrap(result, "postEncryptedIntent");
    },

    // -- Peer Exchange --
    async peerExchange(receiverCellHex, amount) {
      const result = await request<{ exchangeId: string; proofCommitment: string }>(
        base, "/peer-exchange", apiKey, {
          method: "POST",
          body: { receiver_cell: receiverCellHex, amount },
        }
      );
      return unwrap(result, "peerExchange");
    },

    // -- Node --
    async getStatus() {
      const result = await request<{ merkle_root: string; height: number; public_key?: string }>(
        base, "/status", apiKey
      );
      const data = unwrap(result, "getStatus");
      return {
        merkleRoot: data.merkle_root,
        height: data.height,
        publicKey: data.public_key,
      };
    },
  };
}
