# Community-first economics

The user selected a 50-basis-point Beam component: 42 nominal bps community,
5 treasury and 3 founder. Creator payouts are funded from the community
allocation. They are not an extra charge on top of this Beam component.

## Draft AMM v4 routing policy

The new AMM v4 route uses the strict-cap allocation helper carried forward
from PR #23. It rounds the total once with the retained ceiling-fee routine,
caps treasury at floor(notional * 5 / 10,000) and founder at
floor(notional * 3 / 10,000), and assigns the remainder to community.

Global authority and the current creator jointly configure payout
recipients and a creator rate from 0 through 42 bps. Rates exceeding that
budget reject; they are never silently added to the trader's charge or clipped.
The creator share uses ceiling rounding and is deducted from the available
community amount. The remainder stays in the configured community vault.

On 100 SOL notional with a selected 30-bps creator rate, the Beam component is
0.5 SOL: 0.30 creator, 0.12 remaining community, 0.05 treasury and 0.03 founder.
On a one-lamport notional the rounded total is one lamport; with a positive
creator rate it belongs to the creator. Effective dust fees must be disclosed.

Policy updates advance the rights revision and disable routing until both
parties explicitly re-enable it. Historical vault balances keep their original
beneficiaries. Policies start disabled. Both authority and creator must sign to enable the
route. The user specifies minimum swap output, maximum Beam fee and the
expected rights revision. Buys use actual WSOL input as notional; sells use the
observed WSOL output. A failed CPI or exceeded bound rolls back the swap and
all fee transfers together. Fee credits follow successful native-SOL transfers.

The swap uses the user's WSOL and classic-SPL accounts. The Beam fee is paid
separately in native SOL by the signing user. Vault-creation rent and transaction
fees are additional; the maximum Beam fee does not represent a total wallet
debit cap. No token account is silently unwrapped or closed.

Raydium's own pool fee is separate and reflected in swap output. Therefore
0.50% is not an all-in fee quote. Direct Raydium trades bypass this Beam wrapper
and do not fund Beam vaults. The wrapper creates no LP fee entitlement.

## Creator payouts, sharing and CTO

Creator funding is bound to the selected payout wallet or an immutable sharing
plan. Sharing supports at most eight recipients with weights totaling 10,000.
Cumulative floor accounting retains rounding dust and prevents payout changes
from splitting deposits into many small batches. Previous vaults and sharing
plans remain payable to their original beneficiaries.

Normal creator succession requires current creator, successor and Global
authority signatures. Administrative CTO uses the dedicated existing
Global.admin_set_creator_authority role plus successor consent. Both change
future rights only, advance the revision and clear the old donation selection.
Neither can seize old creator/sharing balances, redirect a historical claim,
change Raydium's pool authority, or withdraw pool liquidity.

Donation vaults accept separate explicit deposits to a consented recipient
registration selected by the creator. Revocation blocks new deposits while
old credited funds remain claimable. There is no automatic donation surcharge,
Donate.gg integration, legal charity certification or tax receipt.

## Remaining release and curve boundaries

The inherited curve trade path still uses the retained fee-program CPI and
has not been rehosted onto this new AMM v4 policy. Its current separately
calculated creator fees are an implementation gap, not approval to add creator
fees to the final 50-bps Beam component. Curve fee custody/configuration remains
the separately owned work in the task ledger.

Migration, LP principal custody, Token-2022 graduation, SBF validation,
independent review, authenticated deployment/source matching and a complete
wallet/IDL integration remain release requirements. The retired CPMM
migration remains disabled. This draft deploys nothing.

Community funds remaining after creator allocation may later support holder
rewards and flagship buybacks. Holder eligibility, flagship mint, execution
policy and the provisional 50/50 reward/buyback split still require reviewed
decisions. No volume means no revenue; no price, return or perpetual-income
outcome is promised.
