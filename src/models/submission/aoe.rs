#[derive(Copy)]
#[derive_const(Clone)]
pub enum Aoe {
    Global,
    After(u32),
    Before(u32),
}
