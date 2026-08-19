// The Ledger's ordering and row-state read. No longer implemented here —
// they are `hummingbird_core::decisions::roster::{ledger_row_state,
// last_touched_ms, order_ledger}` (ADR-0025, #141/M3, #532), reached
// through the main-thread wasm seam.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller (`LedgerScreen.tsx`) and
// `ledger-order.test.ts` are untouched, the same pattern `triage-order.ts`
// established at M1-3.

export { ledgerRowState, lastTouchedMs, orderLedger, type LedgerRowState } from "../decisions/seam";
