
pub const FIELD_NAME: &'static str = "f2e16";

pub type Elt = u16;
pub type Wide = u32;

pub const FIELD_BITS: usize = 16;
pub const FIELD_SIZE: usize = 1_usize << FIELD_BITS;

include!("f_log_mul.rs");

#[cfg(table_bootstrap_complete)]
include!("afft.rs");

// must be placed in a separate file, such that the preproc never tries to eval OUT_DIR
// in env which does not exist in the build.rs case
#[cfg(table_bootstrap_complete)]
include!(concat!(env!("OUT_DIR"), "/table_", "f2e16", ".rs"));


/* Needs Cleanup  */

pub type GFSymbol = Elt;
pub const ONEMASK: Elt = (FIELD_SIZE - 1) as Elt;

/// Quotient ideal generator given by tail of irreducible polynomial
pub const GENERATOR: Elt = 0x2D; // x^16 + x^5 + x^3 + x^2 + 1

// impl Additive {
//     pub const ONE: Additive = Additive(???);
// }

// Cantor basis
// pub const BASE: [Elt; FIELD_BITS] =
//    [1_u16, 44234, 15374, 5694, 50562, 60718, 37196, 16402, 27800, 4312, 27250, 47360, 64952, 64308, 65336, 39198];

/// Cantor basis' final element
pub const BASE_FINAL: Elt = 39198;

#[test]
fn embedded_gf256() {
    // We've a leaky to_multiplier abstraction that fucks up zero, so start at 1.
    let mask: Elt = !0xFF;
    for i in 1..=255 {
        let i = Additive(i as Elt).to_multiplier();
        for j in 0..16 {
            let j = Additive(j as Elt);
            assert!(j.mul(i).0 & mask == 0);
        }
    }    
}
