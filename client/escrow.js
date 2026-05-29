
const { AnchorProvider, Program, BN, web3 } = require('@coral-xyz/anchor');
const { SERVER_KEYPAIR, connection, CRYPTAN_MINT, CRYPTAN_FACTOR } = require('./solana');

if (!process.env.ESCROW_PROGRAM_ID) {
  console.warn('[Escrow] ESCROW_PROGRAM_ID not set — smart contract escrow disabled.');
}

const PROGRAM_ID = process.env.ESCROW_PROGRAM_ID
  ? new web3.PublicKey(process.env.ESCROW_PROGRAM_ID)
  : null;

const IDL = {
  "version": "0.1.0", "name": "cryptan_escrow",
  "instructions": [
    { "name": "initializeGame",
      "accounts": [
        { "name": "authority", "isMut": true, "isSigner": true },
        { "name": "escrow",    "isMut": true, "isSigner": false },
        { "name": "vault",     "isMut": true, "isSigner": false },
        { "name": "systemProgram", "isMut": false, "isSigner": false }
      ],
      "args": [
        { "name": "gameId",       "type": "string" },
        { "name": "entryFee",     "type": "u64" },
        { "name": "maxPlayers",   "type": "u8" },
        { "name": "houseFeeBps",  "type": "u16" }
      ]
    },
    { "name": "deposit",
      "accounts": [
        { "name": "player",       "isMut": true,  "isSigner": true },
        { "name": "escrow",       "isMut": true,  "isSigner": false },
        { "name": "vault",        "isMut": true,  "isSigner": false },
        { "name": "systemProgram","isMut": false, "isSigner": false }
      ],
      "args": [{ "name": "gameId", "type": "string" }]
    },
    { "name": "finalize",
      "accounts": [
        { "name": "authority",    "isMut": true,  "isSigner": true },
        { "name": "escrow",       "isMut": true,  "isSigner": false },
        { "name": "vault",        "isMut": true,  "isSigner": false },
        { "name": "systemProgram","isMut": false, "isSigner": false }
      ],
      "args": [
        { "name": "gameId",   "type": "string" },
        { "name": "payouts",  "type": { "vec": { "defined": "PayoutEntry" } } }
      ]
    },
    { "name": "cancel",
      "accounts": [
        { "name": "authority",    "isMut": false, "isSigner": true },
        { "name": "escrow",       "isMut": true,  "isSigner": false },
        { "name": "vault",        "isMut": true,  "isSigner": false },
        { "name": "systemProgram","isMut": false, "isSigner": false }
      ],
      "args": [{ "name": "gameId", "type": "string" }]
    },
    { "name": "initializeTokenGame",
      "accounts": [
        { "name": "authority",     "isMut": true,  "isSigner": true },
        { "name": "tokenMint",     "isMut": false, "isSigner": false },
        { "name": "escrow",        "isMut": true,  "isSigner": false },
        { "name": "tokenVault",    "isMut": true,  "isSigner": false },
        { "name": "tokenProgram",  "isMut": false, "isSigner": false },
        { "name": "systemProgram", "isMut": false, "isSigner": false },
        { "name": "rent",          "isMut": false, "isSigner": false }
      ],
      "args": [
        { "name": "gameId",      "type": "string" },
        { "name": "entryFee",    "type": "u64" },
        { "name": "maxPlayers",  "type": "u8" },
        { "name": "houseFeeBps", "type": "u16" }
      ]
    },
    { "name": "tokenDeposit",
      "accounts": [
        { "name": "player",                  "isMut": true,  "isSigner": true },
        { "name": "tokenMint",               "isMut": false, "isSigner": false },
        { "name": "escrow",                  "isMut": true,  "isSigner": false },
        { "name": "tokenVault",              "isMut": true,  "isSigner": false },
        { "name": "playerAta",               "isMut": true,  "isSigner": false },
        { "name": "tokenProgram",            "isMut": false, "isSigner": false },
        { "name": "associatedTokenProgram",  "isMut": false, "isSigner": false },
        { "name": "systemProgram",           "isMut": false, "isSigner": false }
      ],
      "args": [{ "name": "gameId", "type": "string" }]
    },
    { "name": "tokenFinalize",
      "accounts": [
        { "name": "authority",    "isMut": true,  "isSigner": true },
        { "name": "tokenMint",    "isMut": false, "isSigner": false },
        { "name": "escrow",       "isMut": true,  "isSigner": false },
        { "name": "tokenVault",   "isMut": true,  "isSigner": false },
        { "name": "tokenProgram", "isMut": false, "isSigner": false },
        { "name": "systemProgram","isMut": false, "isSigner": false }
      ],
      "args": [
        { "name": "gameId",   "type": "string" },
        { "name": "payouts",  "type": { "vec": { "defined": "PayoutEntry" } } }
      ]
    },
    { "name": "tokenCancel",
      "accounts": [
        { "name": "authority",    "isMut": false, "isSigner": true },
        { "name": "tokenMint",    "isMut": false, "isSigner": false },
        { "name": "escrow",       "isMut": true,  "isSigner": false },
        { "name": "tokenVault",   "isMut": true,  "isSigner": false },
        { "name": "tokenProgram", "isMut": false, "isSigner": false },
        { "name": "systemProgram","isMut": false, "isSigner": false }
      ],
      "args": [{ "name": "gameId", "type": "string" }]
    },
    { "name": "forceCancel",
      "accounts": [
        { "name": "caller",        "isMut": true,  "isSigner": true },
        { "name": "escrow",        "isMut": true,  "isSigner": false },
        { "name": "vault",         "isMut": true,  "isSigner": false },
        { "name": "systemProgram", "isMut": false, "isSigner": false }
      ],
      "args": [{ "name": "gameId", "type": "string" }]
    },
    { "name": "forceTokenCancel",
      "accounts": [
        { "name": "caller",        "isMut": true,  "isSigner": true },
        { "name": "tokenMint",     "isMut": false, "isSigner": false },
        { "name": "escrow",        "isMut": true,  "isSigner": false },
        { "name": "tokenVault",    "isMut": true,  "isSigner": false },
        { "name": "tokenProgram",  "isMut": false, "isSigner": false },
        { "name": "systemProgram", "isMut": false, "isSigner": false }
      ],
      "args": [{ "name": "gameId", "type": "string" }]
    },
    { "name": "tokenEmergencyRefund",
      "accounts": [
        { "name": "player",                  "isMut": true,  "isSigner": true },
        { "name": "tokenMint",               "isMut": false, "isSigner": false },
        { "name": "escrow",                  "isMut": true,  "isSigner": false },
        { "name": "tokenVault",              "isMut": true,  "isSigner": false },
        { "name": "playerAta",               "isMut": true,  "isSigner": false },
        { "name": "tokenProgram",            "isMut": false, "isSigner": false },
        { "name": "associatedTokenProgram",  "isMut": false, "isSigner": false },
        { "name": "systemProgram",           "isMut": false, "isSigner": false }
      ],
      "args": [{ "name": "gameId", "type": "string" }]
    }
  ],
  "accounts": [
    { "name": "GameEscrow",      "type": { "kind": "struct", "fields": [
        { "name": "authority",    "type": "publicKey" },
        { "name": "gameId",       "type": "string" },
        { "name": "entryFee",     "type": "u64" },
        { "name": "maxPlayers",   "type": "u8" },
        { "name": "playerCount",  "type": "u8" },
        { "name": "houseFeeBps",  "type": "u16" },
        { "name": "players",      "type": { "array": ["publicKey", 4] } },
        { "name": "status",       "type": { "defined": "GameStatus" } },
        { "name": "bump",         "type": "u8" },
        { "name": "vaultBump",    "type": "u8" },
        { "name": "createdAt",    "type": "i64" }
      ]}
    },
    { "name": "TokenGameEscrow", "type": { "kind": "struct", "fields": [
        { "name": "authority",    "type": "publicKey" },
        { "name": "gameId",       "type": "string" },
        { "name": "tokenMint",    "type": "publicKey" },
        { "name": "entryFee",     "type": "u64" },
        { "name": "maxPlayers",   "type": "u8" },
        { "name": "playerCount",  "type": "u8" },
        { "name": "houseFeeBps",  "type": "u16" },
        { "name": "players",      "type": { "array": ["publicKey", 4] } },
        { "name": "status",       "type": { "defined": "GameStatus" } },
        { "name": "bump",         "type": "u8" },
        { "name": "vaultBump",    "type": "u8" },
        { "name": "createdAt",    "type": "i64" }
      ]}
    }
  ],
  "types": [
    { "name": "PayoutEntry", "type": { "kind": "struct", "fields": [
        { "name": "playerIndex", "type": "u8" },
        { "name": "basisPoints", "type": "u16" }
      ]}
    },
    { "name": "GameStatus", "type": { "kind": "enum", "variants": [
        { "name": "Open" }, { "name": "Active" },
        { "name": "Finalized" }, { "name": "Cancelled" }
      ]}
    }
  ],
  "metadata": { "address": "Exm6icPRpt6F6rLRGHqPpjbt87FnFekAZsibqxFccwbs" }
};

let provider = null;
let program  = null;

function getProgram() {
  if (!PROGRAM_ID) return null;
  if (program) return program;
  const serverWallet = {
    publicKey:           SERVER_KEYPAIR.publicKey,
    signTransaction:     async (tx) => { tx.partialSign(SERVER_KEYPAIR); return tx; },
    signAllTransactions: async (txs) => { txs.forEach(t => t.partialSign(SERVER_KEYPAIR)); return txs; },
  };
  provider = new AnchorProvider(connection, serverWallet, { commitment: 'confirmed' });
  program  = new Program(IDL, PROGRAM_ID, provider);
  return program;
}

const TOKEN_PROGRAM_ID       = new web3.PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const ASSOCIATED_TOKEN_PROGRAM_ID = new web3.PublicKey('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJe1bhr');

function getAta(mint, owner) {
  const splToken = require('@solana/spl-token');
  return splToken.getAssociatedTokenAddressSync(mint, owner);
}

function escrowPda(gameId) {
  return web3.PublicKey.findProgramAddressSync(
    [Buffer.from('escrow'), Buffer.from(gameId)], PROGRAM_ID
  );
}
function vaultPda(gameId) {
  return web3.PublicKey.findProgramAddressSync(
    [Buffer.from('vault'), Buffer.from(gameId)], PROGRAM_ID
  );
}

function tokenEscrowPda(gameId) {
  return web3.PublicKey.findProgramAddressSync(
    [Buffer.from('token_escrow'), Buffer.from(gameId)], PROGRAM_ID
  );
}
function tokenVaultPda(gameId) {
  return web3.PublicKey.findProgramAddressSync(
    [Buffer.from('token_vault'), Buffer.from(gameId)], PROGRAM_ID
  );
}


async function initializeGame(gameId, entryFeeSol, maxPlayers, houseFeeRate) {
  const pg = getProgram();
  if (!pg) throw new Error('Escrow program not configured.');
  const entryFeeLamports = new BN(String(Math.round(entryFeeSol * web3.LAMPORTS_PER_SOL)));
  const houseFeeBps = Math.round(houseFeeRate * 10_000);
  const [escrow] = escrowPda(gameId);
  const [vault]  = vaultPda(gameId);
  const sig = await pg.methods
    .initializeGame(gameId, entryFeeLamports, maxPlayers, houseFeeBps)
    .accounts({ authority: SERVER_KEYPAIR.publicKey, escrow, vault, systemProgram: web3.SystemProgram.programId })
    .signers([SERVER_KEYPAIR]).rpc();
  console.log(`[Escrow] SOL game ${gameId} initialised. Sig: ${sig}`);
  return sig;
}

async function verifyDeposit(gameId, walletAddress) {
  const pg = getProgram();
  if (!pg) return false;
  const [escrowAddress] = escrowPda(gameId);
  try {
    const escrow    = await pg.account.gameEscrow.fetch(escrowAddress);
    const playerKey = new web3.PublicKey(walletAddress);
    for (let i = 0; i < escrow.playerCount; i++) {
      if (escrow.players[i].equals(playerKey)) return true;
    }
    return false;
  } catch (e) {
    console.error(`[Escrow] verifyDeposit error for ${gameId}:`, e.message);
    return false;
  }
}

async function finalize(gameId, payouts, recipientWallets) {
  const pg = getProgram();
  if (!pg) throw new Error('Escrow program not configured.');
  const [escrow] = escrowPda(gameId);
  const [vault]  = vaultPda(gameId);
  const remainingAccounts = [
    ...recipientWallets.map(w => ({ pubkey: new web3.PublicKey(w), isSigner: false, isWritable: true })),
    { pubkey: SERVER_KEYPAIR.publicKey, isSigner: false, isWritable: true },
  ];
  const sig = await pg.methods
    .finalize(gameId, payouts)
    .accounts({ authority: SERVER_KEYPAIR.publicKey, escrow, vault, systemProgram: web3.SystemProgram.programId })
    .remainingAccounts(remainingAccounts).signers([SERVER_KEYPAIR]).rpc();
  console.log(`[Escrow] SOL game ${gameId} finalized. Sig: ${sig}`);
  return sig;
}

async function cancelGame(gameId, playerWallets) {
  const pg = getProgram();
  if (!pg) throw new Error('Escrow program not configured.');
  const [escrowAddr] = escrowPda(gameId);
  const [vault]      = vaultPda(gameId);

  const onChain = await pg.account.gameEscrow.fetch(escrowAddr);
  const orderedWallets = [];
  for (let i = 0; i < onChain.playerCount; i++) {
    orderedWallets.push(onChain.players[i].toString());
  }

  const remainingAccounts = orderedWallets.map(w => ({
    pubkey: new web3.PublicKey(w), isSigner: false, isWritable: true,
  }));
  const sig = await pg.methods
    .cancel(gameId)
    .accounts({ authority: SERVER_KEYPAIR.publicKey, escrow: escrowAddr, vault, systemProgram: web3.SystemProgram.programId })
    .remainingAccounts(remainingAccounts).signers([SERVER_KEYPAIR]).rpc();
  console.log(`[Escrow] SOL game ${gameId} cancelled. Sig: ${sig}`);
  return sig;
}

async function fetchEscrow(gameId) {
  const pg = getProgram();
  if (!pg) return null;
  const [escrowAddress] = escrowPda(gameId);
  return pg.account.gameEscrow.fetch(escrowAddress);
}


async function initializeTokenGame(gameId, entryFeeTokens, maxPlayers, houseFeeRate, tokenMint) {
  const pg = getProgram();
  if (!pg) throw new Error('Escrow program not configured.');
  if (!tokenMint) throw new Error('Token mint not configured.');

  const entryFeeBaseUnits = new BN(String(Math.round(entryFeeTokens * CRYPTAN_FACTOR)));
  const houseFeeBps = Math.round(houseFeeRate * 10_000);
  const [escrow]     = tokenEscrowPda(gameId);
  const [tokenVault] = tokenVaultPda(gameId);

  const sig = await pg.methods
    .initializeTokenGame(gameId, entryFeeBaseUnits, maxPlayers, houseFeeBps)
    .accounts({
      authority:     SERVER_KEYPAIR.publicKey,
      tokenMint,
      escrow,
      tokenVault,
      tokenProgram:  TOKEN_PROGRAM_ID,
      systemProgram: web3.SystemProgram.programId,
      rent:          web3.SYSVAR_RENT_PUBKEY,
    })
    .signers([SERVER_KEYPAIR]).rpc();

  console.log(`[Escrow] Token game ${gameId} initialised. Sig: ${sig}`);
  return sig;
}

async function verifyTokenDeposit(gameId, walletAddress) {
  const pg = getProgram();
  if (!pg) return false;
  const [escrowAddress] = tokenEscrowPda(gameId);
  try {
    const escrow    = await pg.account.tokenGameEscrow.fetch(escrowAddress);
    const playerKey = new web3.PublicKey(walletAddress);
    for (let i = 0; i < escrow.playerCount; i++) {
      if (escrow.players[i].equals(playerKey)) return true;
    }
    return false;
  } catch (e) {
    console.error(`[Escrow] verifyTokenDeposit error for ${gameId}:`, e.message);
    return false;
  }
}

async function finalizeToken(gameId, payouts, recipientWallets, tokenMint) {
  const pg = getProgram();
  if (!pg) throw new Error('Escrow program not configured.');
  if (!tokenMint) throw new Error('Token mint not configured.');

  const [escrow]     = tokenEscrowPda(gameId);
  const [tokenVault] = tokenVaultPda(gameId);

  const serverAta = getAta(tokenMint, SERVER_KEYPAIR.publicKey);
  const remainingAccounts = [
    ...recipientWallets.map(w => ({
      pubkey: getAta(tokenMint, new web3.PublicKey(w)), isSigner: false, isWritable: true,
    })),
    { pubkey: serverAta, isSigner: false, isWritable: true },
  ];

  const sig = await pg.methods
    .tokenFinalize(gameId, payouts)
    .accounts({
      authority:     SERVER_KEYPAIR.publicKey,
      tokenMint,
      escrow,
      tokenVault,
      tokenProgram:  TOKEN_PROGRAM_ID,
      systemProgram: web3.SystemProgram.programId,
    })
    .remainingAccounts(remainingAccounts).signers([SERVER_KEYPAIR]).rpc();

  console.log(`[Escrow] Token game ${gameId} finalized. Sig: ${sig}`);
  return sig;
}

async function cancelToken(gameId, playerWallets, tokenMint) {
  const pg = getProgram();
  if (!pg) throw new Error('Escrow program not configured.');
  if (!tokenMint) throw new Error('Token mint not configured.');

  const [escrowAddr] = tokenEscrowPda(gameId);
  const [tokenVault] = tokenVaultPda(gameId);

  const onChain = await pg.account.tokenGameEscrow.fetch(escrowAddr);
  const orderedWallets = [];
  for (let i = 0; i < onChain.playerCount; i++) {
    orderedWallets.push(onChain.players[i].toString());
  }

  const remainingAccounts = orderedWallets.map(w => ({
    pubkey: getAta(tokenMint, new web3.PublicKey(w)), isSigner: false, isWritable: true,
  }));

  const sig = await pg.methods
    .tokenCancel(gameId)
    .accounts({
      authority:     SERVER_KEYPAIR.publicKey,
      tokenMint,
      escrow:        escrowAddr,
      tokenVault,
      tokenProgram:  TOKEN_PROGRAM_ID,
      systemProgram: web3.SystemProgram.programId,
    })
    .remainingAccounts(remainingAccounts).signers([SERVER_KEYPAIR]).rpc();

  console.log(`[Escrow] Token game ${gameId} cancelled. Sig: ${sig}`);
  return sig;
}

async function fetchTokenEscrow(gameId) {
  const pg = getProgram();
  if (!pg) return null;
  const [escrowAddress] = tokenEscrowPda(gameId);
  return pg.account.tokenGameEscrow.fetch(escrowAddress);
}


async function forceCancel(gameId, playerWallets) {
  const pg = getProgram();
  if (!pg) throw new Error('Escrow program not configured.');
  const [escrow] = escrowPda(gameId);
  const [vault]  = vaultPda(gameId);
  const remainingAccounts = playerWallets.map(w => ({
    pubkey: new web3.PublicKey(w), isSigner: false, isWritable: true,
  }));
  const sig = await pg.methods
    .forceCancel(gameId)
    .accounts({ caller: SERVER_KEYPAIR.publicKey, escrow, vault, systemProgram: web3.SystemProgram.programId })
    .remainingAccounts(remainingAccounts).signers([SERVER_KEYPAIR]).rpc();
  console.log(`[Escrow] SOL game ${gameId} force-cancelled (batch refund). Sig: ${sig}`);
  return sig;
}

async function forceTokenCancel(gameId, playerWallets, tokenMint) {
  const pg = getProgram();
  if (!pg) throw new Error('Escrow program not configured.');
  if (!tokenMint) throw new Error('Token mint not configured.');
  const [escrow]     = tokenEscrowPda(gameId);
  const [tokenVault] = tokenVaultPda(gameId);
  const remainingAccounts = playerWallets.map(w => ({
    pubkey: getAta(tokenMint, new web3.PublicKey(w)), isSigner: false, isWritable: true,
  }));
  const sig = await pg.methods
    .forceTokenCancel(gameId)
    .accounts({
      caller: SERVER_KEYPAIR.publicKey,
      tokenMint,
      escrow,
      tokenVault,
      tokenProgram:  TOKEN_PROGRAM_ID,
      systemProgram: web3.SystemProgram.programId,
    })
    .remainingAccounts(remainingAccounts).signers([SERVER_KEYPAIR]).rpc();
  console.log(`[Escrow] Token game ${gameId} force-cancelled (batch refund). Sig: ${sig}`);
  return sig;
}

module.exports = {
  PROGRAM_ID,
  escrowPda, vaultPda,
  initializeGame, verifyDeposit, finalize, cancelGame, forceCancel, fetchEscrow,
  tokenEscrowPda, tokenVaultPda,
  initializeTokenGame, verifyTokenDeposit, finalizeToken, cancelToken, forceTokenCancel, fetchTokenEscrow,
};