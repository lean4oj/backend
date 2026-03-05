import Lean.MonadEnv

namespace CollectLiterals

open Lean

structure State where
  visited : NameSet := {}

abbrev M := ReaderT Environment $ StateM State

@[extern "isMalform_literal"]
opaque _root_.Lean.Literal.isMalform : @& Literal → Bool

@[extern "isMalform_level"]
opaque _root_.Lean.Level.isMalform : @& Level → Bool

@[extern "isMalform_name"]
opaque _root_.Lean.Name.isMalform : @& Name → Bool

def _root_.Lean.Expr.isMalform : Expr → Bool
  | .bvar _ => false
  | .fvar (.mk n)
  | .mvar (.mk n) => n.isMalform
  | .sort u => u.isMalform
  | .const n us => n.isMalform || us.any Level.isMalform
  | .app fn arg => fn.isMalform || arg.isMalform
  | .lam n ty body _
  | .forallE n ty body _ => n.isMalform || ty.isMalform || body.isMalform
  | .letE n ty val body _ => n.isMalform || ty.isMalform || val.isMalform || body.isMalform
  | .lit literal => literal.isMalform
  | .mdata _ expr => expr.isMalform
  | .proj n _ expr => n.isMalform || expr.isMalform

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
    | some (.defnInfo v)   => collectExpr v.type <||> collectExpr v.value
    | some (.thmInfo v)    => collectExpr v.type <||> collectExpr v.value
    | some (.opaqueInfo v) => collectExpr v.type <||> collectExpr v.value
    | some (.quotInfo _)   => pure false
    | some (.ctorInfo v)   => collectExpr v.type
    | some (.recInfo v)    => collectExpr v.type
    | some (.inductInfo v) => collectExpr v.type <||> v.ctors.anyM collect
    | none                 => pure false
