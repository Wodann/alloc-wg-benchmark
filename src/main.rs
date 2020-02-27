use rand::{thread_rng, Rng};
use std::alloc::Layout;
use std::convert::TryInto;
use std::env;
use std::iter::Iterator;
use std::ptr::NonNull;
use std::time::Instant;

use alloc_wg::alloc::{AllocErr, AllocRef, Global, NonZeroLayout};
use bumpalo::Bump;

trait AllocRefV2: Sized {
    fn alloc_non_zst(self, layout: NonZeroLayout) -> Result<NonNull<u8>, AllocErr>;

    #[inline(always)]
    fn alloc_zst(self, layout: Layout) -> Result<NonNull<u8>, AllocErr> {
        // We want to use NonNull::dangling here, but that function uses mem::align_of::<T>
        // internally. For our use-case we cannot call dangling::<T>, since we are not generic
        // over T; we only have access to the Layout of T. Instead we re-implement the
        // functionality here.
        //
        // See https://github.com/rust-lang/rust/blob/9966af3/src/libcore/ptr/non_null.rs#L70
        // for the reference implementation.
        let ptr = layout.align() as *mut u8;
        Ok(unsafe { NonNull::new_unchecked(ptr) })
    }

    #[inline(always)]
    fn alloc(self, layout: Layout) -> Result<NonNull<u8>, AllocErr> {
        if layout.size() == 0 {
            self.alloc_zst(layout)
        } else {
            self.alloc_non_zst(layout.try_into().unwrap())
        }
    }
}

impl<A: AllocRef> AllocRefV2 for &Bump<A> {
    #[inline(always)]
    fn alloc_non_zst(self, layout: NonZeroLayout) -> Result<NonNull<u8>, AllocErr> {
        AllocRef::alloc(self, layout.into())
    }
}

impl AllocRefV2 for Global {
    #[inline(always)]
    fn alloc_non_zst(self, layout: NonZeroLayout) -> Result<NonNull<u8>, AllocErr> {
        AllocRef::alloc(self, layout)
    }
}

fn make_layouts(num: usize, is_zero: bool) -> Vec<Layout> {
    let mut rng = thread_rng();
    (0..num)
        .map(|_| {
            let size: usize = if is_zero {
                rng.gen_range(0, 1)
            } else {
                rng.gen_range(1, 1025)
            };
            let align: usize = 2usize.pow(rng.gen_range(0, 4));
            Layout::from_size_align(size, align).expect("Failed to create layout")
        })
        .collect()
}

fn test_alloc<A: AllocRefV2 + Copy>(a: A, layouts: &[Layout]) {
    let mut allocations = Vec::with_capacity(layouts.len());

    let before = Instant::now();
    for layout in layouts {
        allocations.push(a.alloc(*layout));
    }
    println!("{}", before.elapsed().as_micros());
}

fn test_alloc_zst<A: AllocRefV2 + Copy>(a: A, layouts: &[Layout]) {
    let mut allocations = Vec::with_capacity(layouts.len());

    let before = Instant::now();
    for layout in layouts {
        allocations.push(a.alloc_zst(*layout));
    }
    println!("{}", before.elapsed().as_micros());
}

fn test_alloc_non_zst<A: AllocRefV2 + Copy>(a: A, layouts: &[NonZeroLayout]) {
    let mut allocations = Vec::with_capacity(layouts.len());

    let before = Instant::now();
    for layout in layouts {
        allocations.push(a.alloc_non_zst(*layout));
    }
    println!("{}", before.elapsed().as_micros());
}

fn run_test<A: AllocRefV2 + Copy>(a: A, iters: usize, is_direct: bool, is_zero: bool) {
    let layouts = make_layouts(iters, is_zero);
    if is_direct {
        if is_zero {
            test_alloc_zst(a, &layouts);
        } else {
            test_alloc_non_zst(
                a,
                &layouts
                    .iter()
                    .map(|l| (*l).try_into().unwrap())
                    .collect::<Vec<_>>(),
            )
        }
    } else {
        test_alloc(a, &layouts);
    }
}

/// To run the test, provide the command-line arguments for:
/// - the number of iterations
/// - type of allocator (false: global, true: bump)
/// - type of allocation-size distribution (false: randomly distributed, non-zero, true: zero-sized)
/// - type of function calls (false: branched, true: direct)
///
/// E.g. `cargo run --release -- 10000000 false false true`
fn main() {
    let iters: usize = env::args()
        .nth(1)
        .expect("Expected number of iterations.")
        .parse()
        .unwrap();

    let is_bump: bool = env::args()
        .nth(2)
        .expect("Expected '0' (global) or '1' (bump) to indicate allocator type.")
        .parse()
        .unwrap();

    let is_zero: bool = env::args()
        .nth(3)
        .expect("Expected '0' (randomly distributed, non-zero allocations) or '1' (zero-sized allocations) to indicate allocator type.")
        .parse()
        .unwrap();

    let is_direct: bool = env::args()
        .nth(4)
        .expect("Expected '0' (branched) or '1' (direct) to function call type.")
        .parse()
        .unwrap();

    if is_bump {
        let bump = Bump::with_capacity(1024 * iters);
        run_test(&bump, iters, is_direct, is_zero);
    } else {
        run_test(Global, iters, is_direct, is_zero);
    }
}
