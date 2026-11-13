#[inline(never)]
pub fn leaf() {}

pub fn entry() {
    leaf();
    leaf();
}
