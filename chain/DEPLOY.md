# EVM Bridge Deployment (Base Sepolia)

Deploys PyanaVault and PyanaCredentialGate to Base Sepolia testnet.

## Prerequisites

- Foundry installed: `curl -L https://foundry.paradigm.xyz | bash && foundryup`
- Base Sepolia ETH (faucets: https://www.coinbase.com/faucets/base-ethereum-sepolia-faucet)
- SP1 toolchain for generating the program vkey: `curl -L https://sp1.succinct.xyz | bash && sp1up`

## Setup

```bash
cd chain

# Install forge-std
forge install foundry-rs/forge-std --no-commit

# Copy env and fill in values
cp .env.example .env
# Edit .env with your deployer key, program vkey, etc.
source .env
```

## SP1 Verifier Gateway

The SP1 Verifier Gateway is deployed via deterministic CREATE2 by Succinct Labs on all
EVM chains (same address everywhere). Our contracts call:

```solidity
ISP1Verifier(verifier).verifyProof(vkey, publicValues, proofBytes)
```

Contract addresses (from Succinct docs):
- **SP1VerifierGateway**: `0x3B6041173B80E77f038f3F2C0f9744f04837185e`

Verify the current address at:
https://docs.succinct.xyz/docs/verification/onchain/contract-addresses

If deploying to a chain where the gateway is not yet deployed, you can deploy it yourself
using Succinct's factory or use `SP1MockVerifier` for testing.

## Generate Program VKey

```bash
cd chain/program
cargo prove vkey
# Output: Program Verification Key: 0x00abcdef...
# Use this as PYANA_PROGRAM_VKEY in .env
```

## Build

```bash
forge build
```

## Test

```bash
forge test -vvv
```

## Deploy

```bash
forge script script/Deploy.s.sol \
  --rpc-url base_sepolia \
  --broadcast \
  --verify
```

The script will:
1. Deploy `PyanaVault` (shielded deposit/withdraw, incremental Merkle tree)
2. Deploy `PyanaCredentialGate` (credential-gated minting/voting, VK governance)
3. Log both deployed addresses

## Post-Deployment

1. **Verify on BaseScan** (if `--verify` didn't work):
   ```bash
   forge verify-contract <VAULT_ADDRESS> PyanaVault \
     --chain base-sepolia \
     --constructor-args $(cast abi-encode "constructor(address,bytes32)" $SP1_VERIFIER_ADDRESS $PYANA_PROGRAM_VKEY)

   forge verify-contract <GATE_ADDRESS> PyanaCredentialGate \
     --chain base-sepolia \
     --constructor-args $(cast abi-encode "constructor(address,bytes32,address)" $SP1_VERIFIER_ADDRESS $PYANA_PROGRAM_VKEY <DEPLOYER_ADDRESS>)
   ```

2. **Set trusted federation roots** on the credential gate:
   ```bash
   cast send <GATE_ADDRESS> "setFederationTrust(bytes32,bool)" <FEDERATION_ROOT> true \
     --rpc-url $BASE_SEPOLIA_RPC_URL \
     --private-key $DEPLOYER_PRIVATE_KEY
   ```

3. **Configure the pyana node** with the deployed vault address:
   ```toml
   # In pyana node config
   [bridge]
   rpc_url = "https://sepolia.base.org"
   vault_address = "<VAULT_ADDRESS>"
   ```

## Architecture

```
User deposits ETH/ERC-20 to PyanaVault on Base
  -> Deposit event emitted with noteCommitment
  -> pyana bridge relay observes event
  -> Corresponding note created in pyana's private ledger

User wants to withdraw from pyana to Base:
  -> pyana generates SP1 proof (wraps STARK proof of note validity)
  -> User submits proof to PyanaVault.withdraw()
  -> SP1 Verifier Gateway verifies Groth16 proof on-chain
  -> Vault releases funds to recipient
  -> Nullifier recorded (prevents double-spend)
```
