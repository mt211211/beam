# Beam AMM v4 companion accounts

This draft adds Beam-owned native-SOL creator vaults, creator succession and
donation-recipient vaults alongside AMM v4. It leaves Raydium liquidity and LP
rights untouched. Historical PumpSwap migration/router/CTO APIs reject.

Pool registration requires Global authority and the completed Beam curve's
creator to sign, then verifies the canonical AMM v4 address, owner, 752-byte
layout, status, mint identities and nonce against the pinned source. This is
joint registration of a pool, not a Beam migration receipt or a binary audit.

Funding is an explicit, signer-approved deposit with an expected rights
revision and beneficiary. It is not proof of a swap, automatic collection
from direct Raydium trades, or approval of a new fee. Creator-funding policy,
42/5/3 allocation and actual Raydium fees remain unchanged. A reviewed router
must later disclose and fund any approved charge before the payer signs.

Creator changes require Global authority, current creator and successor
signatures together. Old balances stay in the old creator vault; only future
deposits follow the new creator. Pending transactions with stale revisions
reject. No creator change rewrites a Raydium pool or moves liquidity.

Donation records require Global authority and recipient consent. The creator
selects a record for a pool. Global authority can revoke it; revocation blocks
new deposits and leaves old claims available. Registration is consent and a
local allowlist, not evidence of legal charity status or tax eligibility.

Vaults are scoped by pool, beneficiary and kind. Permissionless claims pay the
immutable beneficiary, preserve rent, and transfer at most credited funds.
Unsolicited lamports are not credited. There is no arbitrary-recipient sweep,
close-vault instruction, LP custody or principal withdrawal capability.

Everything remains development source. Tests use synthetic identities and
valueless balances. SBF/local-bank execution, independent review, a reviewed
swap/migration integration and final recipient/funding policies remain release
requirements. This contribution deploys nothing.

## Routed swaps and creator sharing

The source-pinned orderbook-free SwapBaseInV2 path uses eight Raydium accounts.
A policy jointly configured by Global authority and creator starts disabled.
The trader explicitly bounds output and the Beam fee. The single 50-bps Beam
component pays treasury/founder at strict 5/3-bps caps and funds creator payouts
from the 42-bps community allocation. Direct Raydium swaps bypass that wrapper.
See ECONOMICS.md for WSOL, native-SOL fees, rent and curve-path boundaries.

A creator can select a consented payout wallet or an immutable sharing plan
with up to eight beneficiaries. Plans retain cumulative payout history and
rounding dust. Changing the payout advances the revision and affects future
deposits only. Permissionless distribution cannot substitute recipients.

Normal succession requires three signatures. A separate administrative CTO
path preserves the existing dedicated admin_set_creator_authority role and
requires successor consent; it does not need an abandoned creator's signature.
It cannot move or rewrite historical creator/sharing obligations.

Global authority and the current creator may jointly update future fee rates
or fee recipients. Each update advances the revision and disables routing
until both explicitly re-enable it. Old vault obligations do not change.

## Graduated quote-chain accounts

Active ancestors retain their one-account format. A graduated ancestor uses
five consecutive accounts: its Beam curve, registered rights, AMM v4 pool,
coin vault and native WSOL vault. The pool must be canonical and SwapOnly;
vault owners, mints, authority, native backing and pending PnL are checked.
Rights must be unpaused and bound to the ancestor mint. Old PumpSwap pool
bytes are always rejected. This is spot pricing, not a TWAP or secure oracle.
