#![feature(register_tool)]
#![register_tool(rapx)]

// The resolved callee of `compute` is a closure DefId. It must not be queried
// with TyCtxt::fn_sig while classifying unrelated Range methods.
#[rapx::verify]
pub fn closure_before_unsafe() -> u32 {
    let compute = |value: u32| value + 1;
    let value = compute(1);
    unsafe { std::ptr::read(&value) }
}
