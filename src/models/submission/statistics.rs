use serde::Deserialize;

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Type {
    Fastest,
    MinMemory,
    MinAnswerSize,
    Earliest,
}
