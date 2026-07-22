#[derive(Copy, Debug)]
#[derive_const(Clone, PartialEq, Eq)]
pub enum Mode {
    Read,
    Write,
}
