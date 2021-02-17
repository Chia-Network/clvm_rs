use crate::cost::Cost;

#[derive(Debug, Clone, PartialEq)]
pub struct EvalErr<T>(pub T, pub String);

#[derive(Debug, PartialEq)]
pub struct Reduction<T>(pub Cost, pub T);

pub type Response<T> = Result<Reduction<T>, EvalErr<T>>;
