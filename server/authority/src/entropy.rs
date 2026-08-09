//! The entropy seam: token minting needs random bytes, and the pure crate
//! cannot depend on a randomness runtime (`getrandom`'s wasm backend drags
//! in js-sys, which the no-bindings guard forbids). Same discipline as
//! [`crate::Sql`] — the shim injects the real source, fixtures inject
//! deterministic bytes.

pub trait Entropy {
    fn fill(&self, buf: &mut [u8]);
}
