// The frontier's facet filter (#403). No longer implemented here — it is
// `hummingbird_core::decisions::frontier` (ADR-0025, #141/M1-3: `Facet`,
// `matches_facets`, `apply_facets`, `toggle_facet`, `facet_count`,
// `contexts_of`, `NO_CONTEXT`), reached through the main-thread wasm seam.
// `SIZES`/`ENERGIES` were this module's own hand-typed copy of the wire's
// size/energy vocabulary — the one the M1-2 review flagged as the
// surviving unpinned copy — and now come from the seam too, pinned against
// `hummingbird_core::decisions::vocabulary` the same way `field-vocabulary
// .ts`'s `SIZE_OPTIONS`/`ENERGY_OPTIONS` already are.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller (`FrontierColumns.tsx`) and
// `frontier-facets.test.ts` are untouched, the same pattern
// `capture-validation.ts` established at M1-1.

export {
  applyFacets,
  contextsOf,
  ENERGIES,
  facetCount,
  FACETS,
  matchesFacets,
  NO_CONTEXT,
  NO_FACETS,
  SIZES,
  toggleFacet,
  URGENCIES,
  type Facet,
  type FacetSelection,
} from "../decisions/seam";
