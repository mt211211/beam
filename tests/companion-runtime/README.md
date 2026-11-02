# AMM v4 companion local-bank tests

Runs the actual Beam Anchor instructions with synthetic accounts and valueless
balances in solana-program-test. The Raydium pool is a source-layout fixture.
A deliberately synthetic AMM processor performs real SPL-token CPIs at a fixed
test price to exercise Beam's wiring and transaction rollback. This is not
real Raydium pricing/execution parity, SBF, a live network, or a deployment.

From the repository root, with Rust 1.90.0:

    cargo test --locked --manifest-path tests/companion-runtime/Cargo.toml
    cargo test --locked --manifest-path tests/companion-runtime/Cargo.toml --features experimental-raydium

The single end-to-end scenario covers:

- Pool registration authority and actual System-program vault funding.
- Normal three-party creator succession and dedicated-role administrative CTO.
- Stale revisions, old/new creator payouts, surplus and repeated claims.
- Donation registration/selection/revocation with historical claims preserved.
- Configured AMM v4 buy/sell CPIs, exact token deltas, one 50-bps Beam component,
  creator funding from community, fee caps, slippage and rollback after CPIs.
- Creator sharing with fixed recipients, cumulative dust and replay handling.
- Joint policy updates, deposit pause and immutable historical obligations.

The bank asserts balances and account bytes, including failure rollbacks.
Pool lamports/data are unchanged by the mock, while pool token vaults move
only through authorized swaps. No LP custody or withdrawal instruction exists.

Windows builds need MSVC C++ tools and a native Windows Perl for the pinned
vendored OpenSSL dependency. Set PERL to that executable. A writable temporary
CARGO_TARGET_DIR can be used if normal Documents-folder application protections
prevent compiler build scripts from writing; no security settings need changing.

Production still needs independent review, real Raydium and SBF evidence,
migration/LP policy, wallet/IDL integration and release authorization.
