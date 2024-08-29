use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{ self, Mint, TokenAccount, TokenInterface, TransferChecked };

declare_id!("GFdLg11UBR8ZeePW43ZyD1gY4z4UQ96LPa22YBgnn4z8");
#[program]
pub mod vesting {
    use super::*;

    pub fn create_vesting_account(
        ctx: Context<CreateVestingAccount>,
        company_name: String
    ) -> Result<()> {
        // The * operator is used to dereference an account, in this case its the account that the vesting_account variable points to.
        // To modify an account, you need to dereference the account reference. 
        // This dereference operator tells Rust that you want to work with the actual account data, not just the reference.
        // So now you can update values saved to the account state of the vesting_account.
        *ctx.accounts.vesting_account = VestingAccount {
            owner: ctx.accounts.signer.key(),
            mint: ctx.accounts.mint.key(),
            treasury_token_account: ctx.accounts.treasury_token_account.key(),
            company_name,
            treasury_bump: ctx.bumps.treasury_token_account,
            bump: ctx.bumps.vesting_account,
        };

        Ok(())
    }

    pub fn create_employee_vesting(
        ctx: Context<CreateEmployeeAccount>,
        start_time: i64,
        end_time: i64,
        total_amount: i64,
        cliff_time: i64
    ) -> Result<()> {
        *ctx.accounts.employee_account = EmployeeAccount {
            beneficiary: ctx.accounts.beneficiary.key(),
            start_time,
            end_time,
            total_amount,
            total_withdrawn: 0,
            cliff_time,
            vesting_account: ctx.accounts.vesting_account.key(),
            bump: ctx.bumps.employee_account,
        };

        Ok(())
    }

    pub fn claim_tokens(ctx: Context<ClaimTokens>, _company_name: String) -> Result<()> {
        // &mut is used to borrow data with the intent to modify it
        let employee_account = &mut ctx.accounts.employee_account;


        // Earlier we used the dereference operator so lets compare the two. 
        // Dereferencing is used when you need to manipulate the actual data stored at the reference's location
        // This is less common in high-level operations and is typically seen in scenarios where an explicit change 
        // of the data in the account is necessary, such as initializing or resetting the account data.
        // Earlier we were creating accounts and initializing its data, which is why we used the dereference operator.
        // However, borrowing a mutable reference, you can pass employee_account around in your function or to other 
        // functions while retaining the ability to modify the original data. This is useful when you want to perform multiple 
        // operations on the data, or when the data needs to be accessed or modified by multiple parts of your program.
        // Mutable references are safer in a multi-threaded context because Rust's compiler enforces rules that prevent data races. 
        // Dereferencing needs careful handling to avoid issues like data corruption or undefined behavior, especially in a 
        // system-level programming context.

        let now = Clock::get()?.unix_timestamp;

        // Check if the current time is before the cliff time
        if now < employee_account.cliff_time {
            return Err(ErrorCode::ClaimNotAvailableYet.into());
        }

        // Now when running calculations in rust, you need to take into consideration underflow amd overflow errors.
        // Underflow occurs when a calculation results in a number that is smaller than the minimum value that can be stored in the data type.
        // Overflow occurs when a calculation results in a number that is larger than the maximum value that can be stored in the data type.

        // Calculate the vested amount
        // saturating_sub ensures that the subtraction does not go below zero, which can prevent underflow errors. 
        let time_since_start = now.saturating_sub(employee_account.start_time);
        let total_vesting_time = employee_account.end_time.saturating_sub(
            employee_account.start_time
        );
        if total_vesting_time == 0 {
            return Err(ErrorCode::InvalidVestingPeriod.into());
        }
        let vested_amount = if now >= employee_account.end_time {
            employee_account.total_amount
        } else {
            // Perform a checked multiplication to handle possible overflow
            match employee_account.total_amount.checked_mul(time_since_start) {
                Some(product) => {
                    // Safe to do the division after successful multiplication
                    product / total_vesting_time
                },
                None => {
                    // Handle overflow case, e.g., by logging or returning an error
                    return Err(ErrorCode::CalculationOverflow.into());
                }
            }
        };

        //Calculate the amount that can be withdrawn
        let claimable_amount = vested_amount.saturating_sub(employee_account.total_withdrawn);
        // Check if there is anything left to claim
        if claimable_amount == 0 {
            return Err(ErrorCode::NothingToClaim.into());
        }

        // Now we can transfer the tokens to the employee, this invloves a CPI call, which is a cross-program invocation.
        // 
        let transfer_cpi_accounts = TransferChecked {
            from: ctx.accounts.treasury_token_account.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.employee_token_account.to_account_info(),
            authority: ctx.accounts.treasury_token_account.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        // When transferring tokens using the SPL Token program, the transfer instruction must be signed by the owner of the tokens or an authorized delegate.
        // In our case, the authority is the treasury_token_account, which is the account that holds the tokens.
        // So we need to define the seeds for this account to sign the transfer instruction.

        let signer_seeds: &[&[&[u8]]] = &[
            &[
                b"vesting_treasury",
                ctx.accounts.vesting_account.company_name.as_ref(),
                &[ctx.accounts.vesting_account.treasury_bump],
            ],
        ];
        let cpi_context = CpiContext::new(cpi_program, transfer_cpi_accounts).with_signer(
            signer_seeds
        );
        // you're specifying that the CPI call should be signed by an account derived from the provided seeds. 
        let decimals = ctx.accounts.mint.decimals;
        token_interface::transfer_checked(cpi_context, claimable_amount as u64, decimals)?;
        employee_account.total_withdrawn += claimable_amount;
        Ok(())
    }
}

// In Anchor, each account used in a transaction is declared in a context structure with specific traits 
// that dictate how the account can be accessed (like whether it's mutable or immutable).
// When the function is called, anchor passes references  to these accounts into the function. 
// These are not the actual accounts but pointers to where the account data is stored.
#[derive(Accounts)]
#[instruction(company_name: String)]
pub struct CreateVestingAccount<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        init,
        space = 8 + VestingAccount::INIT_SPACE,
        payer = signer,
        seeds = [company_name.as_ref()],
        bump
    )]
    pub vesting_account: Account<'info, VestingAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        init,
        token::mint = mint,
        token::authority = treasury_token_account,
        payer = signer,
        seeds = [b"vesting_treasury", company_name.as_bytes()],
        bump
    )]
    pub treasury_token_account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateEmployeeAccount<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    pub beneficiary: SystemAccount<'info>,
    #[account(has_one = owner)]
    pub vesting_account: Account<'info, VestingAccount>,
    #[account(
        init,
        space = 8 + EmployeeAccount::INIT_SPACE,
        payer = owner,
        seeds = [b"employee_vesting", beneficiary.key().as_ref(), vesting_account.key().as_ref()],
        bump
    )]
    pub employee_account: Account<'info, EmployeeAccount>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(company_name: String)]
// The has_one constraint is used within the #[account] attribute macro to assert that the specified field of a data structure 
// (usually an account in this context) points to a specific account.
pub struct ClaimTokens<'info> {
    #[account(mut)]
    pub beneficiary: Signer<'info>,
    #[account(
        mut,
        seeds = [b"employee_vesting", beneficiary.key().as_ref(), vesting_account.key().as_ref()],
        bump = employee_account.bump,
        has_one = beneficiary,
        has_one = vesting_account
    )]
    pub employee_account: Account<'info, EmployeeAccount>,
    // For example:  This constraint asserts that the treasury_token_account field within the vesting_account struct must match 
    // the account passed in the instruction context under a variable expected to be named treasury_token_account. 
    // This enforces that the specific vesting_account is linked to this particular treasury_token_account.
    #[account(
        mut,
        seeds = [company_name.as_ref()],
        bump = vesting_account.bump,
        has_one = treasury_token_account,
        has_one = mint
    )]
    pub vesting_account: Account<'info, VestingAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub treasury_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = beneficiary,
        associated_token::mint = mint,
        associated_token::authority = beneficiary,
        associated_token::token_program = token_program
    )]
    pub employee_token_account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// The account attribute is applied to a struct to designate it as a solana account.
#[account]
// Derive macro generating an impl of the traits Debug and Init Space. 
// The InitSpace trait is used to calculate the space required to store the account on chain.
// The Debug trait provides a way to output the internal state of the type for debugging purposes
#[derive(InitSpace, Debug)]
pub struct VestingAccount {
    pub owner: Pubkey,
    pub mint: Pubkey,
    pub treasury_token_account: Pubkey,
    #[max_len(50)]
    pub company_name: String,
    pub treasury_bump: u8,
    pub bump: u8,
}

#[account]
#[derive(InitSpace, Debug)]
pub struct EmployeeAccount {
    pub beneficiary: Pubkey,
    pub start_time: i64,
    pub end_time: i64,
    pub total_amount: i64,
    pub total_withdrawn: i64,
    pub cliff_time: i64,
    pub vesting_account: Pubkey,
    pub bump: u8,
}

// This error code attribute is applied to an enum to designate it as a collection of error codes.
// This generates a result of type T and an error type that can be used to return errors from the program.
#[error_code]
pub enum ErrorCode {
    #[msg("Claiming is not available yet.")]
    ClaimNotAvailableYet,
    #[msg("There is nothing to claim.")]
    NothingToClaim,
    #[msg("Invalid vesting period.")]
    InvalidVestingPeriod,
    #[msg("Calculation overflow.")]
    CalculationOverflow,
}
