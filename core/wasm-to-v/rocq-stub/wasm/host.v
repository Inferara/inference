(* Wasm.host -- vendored signature stub for the Inference proof-mode Rocq
   output contract. Signatures only; no semantics. See README.md.

   Emitted theorems are stated under `Section Host. Context `{ho: host}.`, and
   the real `ValidSpec` is a host-parameterized predicate. An empty class
   reproduces that binder shape without modelling any host operations. In the
   real WasmCert library `host` lives in `Wasm.host`; keeping it under the same
   logical path lets the emitted `From Wasm Require Import ... host.` resolve
   against either the stub or the real library unchanged. *)

Class host : Type := { }.
