// No imports needed: web3, anchor, pg and more are globally available
import * as anchor from '@coral-xyz/anchor';
import { BankrunProvider } from 'anchor-bankrun';
import { TOKEN_PROGRAM_ID } from '@solana/spl-token';
import { BN, Program } from '@coral-xyz/anchor';

import { startAnchor, Clock, BanksClient } from 'solana-bankrun';

import { createMint, mintTo } from 'spl-token-bankrun';
import { PublicKey, Keypair } from '@solana/web3.js';
import NodeWallet from '@coral-xyz/anchor/dist/cjs/nodewallet';

import IDL from '../target/idl/vesting.json';
import { Vesting } from '../target/types/vesting';

describe('Test', () => {
  const companyName = 'Company';
  let beneficiary: Keypair;
  let vestingAccount: PublicKey;
  let treasuryTokenAccount: PublicKey;
  let employeeAccount: PublicKey;

  it('Test Vesting Smart Contract', async () => {
    // set up bankrun
    const context = await startAnchor(
      '',
      [{ name: 'vesting', programId: new PublicKey(IDL.address) }],
      []
    );
    const provider = new BankrunProvider(context);

    anchor.setProvider(provider);

    const program = new Program<Vesting>(IDL as Vesting, provider);

    const banksClient: BanksClient = context.banksClient;

    const employer = provider.wallet.payer;

    // Create a new mint
    const mint = await createMint(
      banksClient,
      employer,
      employer.publicKey,
      null,
      2
    );

    // Generate a new keypair for the beneficiary
    // const beneficiary = Keypair.generate();
    // const beneficiaryWallet = new NodeWallet(beneficiary);
    // const beneficiaryProvider = new BankrunProvider(context, beneficiaryWallet);

    const beneficiary = new anchor.web3.Keypair();

    // Derive PDAs
    [vestingAccount] = PublicKey.findProgramAddressSync(
      [Buffer.from(companyName)],
      program.programId
    );

    [treasuryTokenAccount] = PublicKey.findProgramAddressSync(
      [Buffer.from('vesting_treasury'), Buffer.from(companyName)],
      program.programId
    );

    [employeeAccount] = PublicKey.findProgramAddressSync(
      [
        Buffer.from('employee_vesting'),
        beneficiary.publicKey.toBuffer(),
        vestingAccount.toBuffer(),
      ],
      program.programId
    );

    const tx = await program.methods
      .createVestingAccount(companyName)
      .accounts({
        signer: employer.publicKey,
        mint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc({ commitment: 'confirmed' });

    const vestingAccountData = await program.account.vestingAccount.fetch(
      vestingAccount,
      'confirmed'
    );
    console.log(
      'Vesting Account Data:',
      JSON.stringify(vestingAccountData, null, 2)
    );

    const amount = 10_000 * 10 ** 9;
    // Fund treasuryTokenAccount
    const mintTx = await mintTo(
      banksClient,
      employer,
      mint,
      treasuryTokenAccount,
      employer,
      amount
    );

    console.log('Mint to Treasury Transaction Signature:', mintTx);
    console.log('Create Vesting Account Transaction Signature:', tx);

    const tx2 = await program.methods
      .createEmployeeVesting(new BN(0), new BN(100), new BN(100), new BN(0))
      .accounts({
        beneficiary: beneficiary.publicKey,
        vestingAccount,
      })
      .rpc({ commitment: 'confirmed', skipPreflight: true });

    console.log('Create Employee Account Transaction Signature:', tx2);

    const currentClock = await banksClient.getClock();
    context.setClock(
      new Clock(
        currentClock.slot,
        currentClock.epochStartTimestamp,
        currentClock.epoch,
        currentClock.leaderScheduleEpoch,
        1000n
      )
    );

    const tx3 = await program.methods
      .claimTokens(companyName)
      .accounts({
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([beneficiary])
      .rpc({ commitment: 'confirmed' });

    console.log('Claim Tokens transaction signature', tx3);
  });
});
