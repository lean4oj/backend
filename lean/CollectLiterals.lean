import Lean.MonadEnv

namespace CollectLiterals

open Lean

structure State where
  visited : NameSet := {}

abbrev M := ReaderT Environment $ StateM State

@[extern "isMalform_literal"]
opaque _root_.Lean.Literal.isMalform : @& Literal → Bool

def _root_.Lean.Expr.isMalform : Expr → Bool
  | .lit literal => literal.isMalform
  | .app fn arg => fn.isMalform || arg.isMalform
  | .lam _ ty body _
  | .forallE _ ty body _ => ty.isMalform || body.isMalform
  | .letE _ ty val body _ => ty.isMalform || val.isMalform || body.isMalform
  | .mdata _ expr
  | .proj _ _ expr => expr.isMalform
  | _ => false

partial def collect (c : Name) : M Bool := do
  let collectExpr (e : Expr) : M Bool := if e.isMalform then pure true else e.getUsedConstants.anyM collect
  let s ← get
  if s.visited.contains c then
    pure false
  else do
    modify fun s => { s with visited := s.visited.insert c }
    let env ← read
    match env.checked.get.find? c with
    | some (.axiomInfo v)  => collectExpr v.type
    | some (.defnInfo v)   => collectExpr v.type *> collectExpr v.value
    | some (.thmInfo v)    => collectExpr v.type *> collectExpr v.value
    | some (.opaqueInfo v) => collectExpr v.type *> collectExpr v.value
    | some (.quotInfo _)   => pure false
    | some (.ctorInfo v)   => collectExpr v.type
    | some (.recInfo v)    => collectExpr v.type
    | some (.inductInfo v) => collectExpr v.type *> v.ctors.anyM collect
    | none                 => pure false
