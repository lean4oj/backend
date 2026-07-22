use serde::Deserialize;

#[derive(Copy)]
#[derive_const(Clone, PartialEq, Eq, Deserialize)]
pub enum Type {
    Fastest,
    MinMemory,
    MinAnswerSize,
    Earliest,
}
