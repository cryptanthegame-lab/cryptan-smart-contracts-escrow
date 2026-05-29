# Cryptan Escrow — Open Source Smart Contract

This repository contains the on-chain escrow program that secures all wager rooms on [Cryptan](https://cryptan.world) — a competitive Catan-style board game built on Solana.

Every SOL and $CRYPTAN wager is locked in this program. **The server cannot steal, redirect, or withhold funds.** This code is published so players and investors can verify that claim themselves.

> **Proprietary notice:** Only the smart contract (`program/lib.rs`) and its integration layer (`client/escrow.js`) are released under the MIT license. All other Cryptan game code, server infrastructure, assets, and design are proprietary. All rights reserved. Copying or reusing them without written permission from Cryptan is prohibited.

---

## Program ID

```
Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs
```

Verify it live on Solana Explorer:
- [Mainnet](https://explorer.solana.com/address/Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs)
- [Devnet](https://explorer.solana.com/address/Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs?cluster=devnet)

---

## What This Program Does

When two or more players enter a wager room on Cryptan, their entry fees are deposited into a **Program Derived Address (PDA) vault** controlled exclusively by this smart contract. No private key controls the vault — only the program's own logic can move the tokens inside it.

The program supports two independent escrow tracks:

| Track | Token | Fee | Instructions |
|---|---|---|---|
| SOL | Native SOL (lamports) | 10% | `initialize_game` · `deposit` · `finalize` · `cancel` · `emergency_refund` · `force_cancel` |
| CRYPTAN | SPL token ($CRYPTAN mint) | 5% | `initialize_token_game` · `token_deposit` · `token_finalize` · `token_cancel` · `token_emergency_refund` · `force_token_cancel` |

---

## Security Guarantees — Proven by the Code

### 1. The server cannot steal funds

The vault is a PDA. Its only signer seeds are `["vault", game_id]` (SOL) or `["token_vault", game_id]` (CRYPTAN). No private key exists for it. Funds can only move through the program's own instructions.

```rust
let vault_seeds: &[&[u8]] = &[VAULT_SEED, game_id_bytes, &[escrow.vault_bump]];
let signer_seeds = &[vault_seeds];
// Only invoke_signed with these seeds can move lamports out of the vault.
```

### 2. The house fee is fixed and enforced on-chain

The fee rate (`house_fee_bps`) is set once at game initialization and locked into the escrow account. The server cannot change it mid-game or take more than agreed.

```rust
let house_fee = vault_lamports
    .checked_mul(escrow.house_fee_bps as u64)?
    .checked_div(10_000)?;
let distributable = vault_lamports.checked_sub(house_fee)?;
```

SOL games: `house_fee_bps = 1000` (10%). CRYPTAN games: `house_fee_bps = 500` (5%). Both are verified at initialization and cannot exceed 20% (`HouseFeeTooHigh` guard).

### 3. Payouts go to verified player wallets only

`finalize` checks that every recipient in `remaining_accounts` matches the registered `escrow.players[idx]` address. The server cannot redirect winnings to an arbitrary wallet.

```rust
let recipient = &remaining[i];
require!(recipient.key() == escrow.players[idx], EscrowError::MissingAccount);
```

### 4. A game can only be finalized or cancelled once

`GameStatus` is an on-chain enum. Once set to `Finalized` or `Cancelled`, all money instructions revert immediately.

```rust
require!(
    escrow.status != GameStatus::Finalized &&
    escrow.status != GameStatus::Cancelled,
    EscrowError::AlreadyFinalized
);
```

### 5. No player can be double-charged

The `AlreadyDeposited` guard checks every registered wallet before accepting a deposit.

```rust
for i in 0..(escrow.player_count as usize) {
    require!(escrow.players[i] != player_key, EscrowError::AlreadyDeposited);
}
```

### 6. Emergency escape hatch — no server required

If Cryptan's servers go permanently offline and a game is never resolved, any deposited player can call `emergency_refund` (SOL) or `token_emergency_refund` (CRYPTAN) directly from their Phantom wallet after **24 hours**. No authority signature is needed. The timelock is hardcoded in the contract:

```rust
const EMERGENCY_TIMEOUT_SECS: i64 = 24 * 60 * 60; // 86 400 seconds

require!(
    now >= escrow.created_at + EMERGENCY_TIMEOUT_SECS,
    EscrowError::TimelockNotExpired
);
```

Each player calls this for themselves and receives exactly their `entry_fee` back. The server gets nothing if the game never completed.

There is also a batch variant (`force_cancel` / `force_token_cancel`) that any single deposited player can call to refund **all** players at once after 24 hours — no coordination required.

### 7. Only deposited players can use the escape hatch

Random wallets cannot drain the vault by calling emergency instructions. The contract verifies the caller is registered in `escrow.players[]`.

```rust
let player_idx = escrow.players[..escrow.player_count as usize]
    .iter()
    .position(|pk| *pk == caller)
    .ok_or(EscrowError::NotAPlayer)?;
```

---

## How to Verify Yourself

You do not need to trust us. You can verify the program directly on-chain without running any Cryptan code.

**1. Confirm the program is deployed at the declared ID**

```bash
solana program show Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs
```

**2. Fetch the IDL reconstructed from on-chain bytecode**

```bash
anchor idl fetch Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs
```

**3. Inspect a live escrow account for any active wager room**

```bash
# Replace ROOMID with an active game's 6-character room code
solana account $(solana find-program-derived-address \
  Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs \
  "escrow" "ROOMID")
```

**4. Confirm a vault balance matches the pool shown in-game**

```bash
solana balance $(solana find-program-derived-address \
  Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs \
  "vault" "ROOMID")
```

---

## Repository Contents

| File | Description |
|---|---|
| `program/lib.rs` | The complete Anchor smart contract source |
| `client/escrow.js` | The Node.js integration layer — shows exactly how the server calls each instruction |

The game server, matchmaking logic, authentication, and database code are not published here. Those components do not control funds — the on-chain program does, and that program is fully visible above.

---

## Build & Deploy

Built with [Anchor](https://anchor-lang.com) `0.29.0`.

```toml
# Cargo.toml
anchor-lang = "0.29.0"
anchor-spl  = "0.29.0"
```

```bash
anchor build
anchor deploy --program-id Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs
```

---

## Audit Status

This contract has not undergone a formal third-party audit. The source is published so the community can review it. If you find a vulnerability, please contact us responsibly at **cryptan.thegame@gmail.com** before public disclosure.

---

## License

The smart contract (`program/lib.rs`) and integration layer (`client/escrow.js`) are released under the **MIT License**.

All other Cryptan game code, server infrastructure, branding, and assets are proprietary. © Cryptan. All rights reserved.

---

*Cryptan — Play. Wager. Win. Built on Solana.*
