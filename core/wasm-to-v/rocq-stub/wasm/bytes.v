(* Wasm.bytes -- vendored signature stub for the Inference proof-mode Rocq
   output contract. Signatures only; no semantics. See README.md for purpose,
   drift risk, and the swap-to-real-library follow-up. *)

Require Import String.
Require Import List.
Require Import ZArith.

(* The emitted `Mi`/`Me` helpers store import/export module and field names as a
   `list byte` via `list_byte_of_string`. We declare our own opaque `byte`
   rather than reuse the standard library's `Byte.byte`, whose module path was
   renamed across the Coq -> Rocq transition (`Coq.*` vs `Stdlib.*`); an opaque
   parameter keeps the stub compiling on both apt Coq 8.x and brew Rocq 9.x. *)
Parameter byte : Type.

Parameter list_byte_of_string : string -> list byte.

(* Data-segment contents reach the `.v` as byte literals. `byte_scope`, the
   `encode` its notations abbreviate, and the notations themselves mirror the
   interface of wasm-verifier's `coq-wasm` dependency, where `byte` is
   CompCert's `Integers.byte` and `encode` builds one from a `Z` via
   `Byte.repr`. Here `encode` stays an opaque parameter of the opaque `byte`
   above: the spellings and the byte result type are the contract an emitted
   `moddata_init` has to meet, not the arithmetic behind the value, so the stub
   type-checks the emitted shape without modelling byte values.

   The two-digit uppercase hex notations below stop at the 244 values the
   dependency declares. Its notation block is hand-written and skips twelve
   values -- `#12` .. `#19` and `#1C` .. `#1F` -- so those twelve have no
   literal syntax at all there and reach the `.v` as `encode` applications
   instead. Declaring them here anyway would make this mirror accept a module
   the backend rejects, which is the false green the gate exists to prevent.
   The dependency's block also abbreviates the single hex digits `#A` .. `#F`
   for use inside its own byte arithmetic; an emitted module never spells one,
   so this mirror omits them.

   The dependency opens `byte_scope` at module level, before its notation
   block, and so leaves it open for importers; this mirror deliberately does
   not. With the scope left closed, an emitted module compiles here only if it
   opens the scope its own byte notations need, instead of inheriting an open
   scope from a `Require Import` chain this stub cannot reproduce faithfully. *)
Declare Scope byte_scope.
Delimit Scope byte_scope with byte.

Parameter encode : Z -> byte.

Notation "#00" := (encode 0%Z) : byte_scope.
Notation "#01" := (encode 1%Z) : byte_scope.
Notation "#02" := (encode 2%Z) : byte_scope.
Notation "#03" := (encode 3%Z) : byte_scope.
Notation "#04" := (encode 4%Z) : byte_scope.
Notation "#05" := (encode 5%Z) : byte_scope.
Notation "#06" := (encode 6%Z) : byte_scope.
Notation "#07" := (encode 7%Z) : byte_scope.
Notation "#08" := (encode 8%Z) : byte_scope.
Notation "#09" := (encode 9%Z) : byte_scope.
Notation "#0A" := (encode 10%Z) : byte_scope.
Notation "#0B" := (encode 11%Z) : byte_scope.
Notation "#0C" := (encode 12%Z) : byte_scope.
Notation "#0D" := (encode 13%Z) : byte_scope.
Notation "#0E" := (encode 14%Z) : byte_scope.
Notation "#0F" := (encode 15%Z) : byte_scope.
Notation "#10" := (encode 16%Z) : byte_scope.
Notation "#11" := (encode 17%Z) : byte_scope.
(* The dependency's notation block skips #12 .. #19 and #1C .. #1F. *)
Notation "#1A" := (encode 26%Z) : byte_scope.
Notation "#1B" := (encode 27%Z) : byte_scope.
Notation "#20" := (encode 32%Z) : byte_scope.
Notation "#21" := (encode 33%Z) : byte_scope.
Notation "#22" := (encode 34%Z) : byte_scope.
Notation "#23" := (encode 35%Z) : byte_scope.
Notation "#24" := (encode 36%Z) : byte_scope.
Notation "#25" := (encode 37%Z) : byte_scope.
Notation "#26" := (encode 38%Z) : byte_scope.
Notation "#27" := (encode 39%Z) : byte_scope.
Notation "#28" := (encode 40%Z) : byte_scope.
Notation "#29" := (encode 41%Z) : byte_scope.
Notation "#2A" := (encode 42%Z) : byte_scope.
Notation "#2B" := (encode 43%Z) : byte_scope.
Notation "#2C" := (encode 44%Z) : byte_scope.
Notation "#2D" := (encode 45%Z) : byte_scope.
Notation "#2E" := (encode 46%Z) : byte_scope.
Notation "#2F" := (encode 47%Z) : byte_scope.
Notation "#30" := (encode 48%Z) : byte_scope.
Notation "#31" := (encode 49%Z) : byte_scope.
Notation "#32" := (encode 50%Z) : byte_scope.
Notation "#33" := (encode 51%Z) : byte_scope.
Notation "#34" := (encode 52%Z) : byte_scope.
Notation "#35" := (encode 53%Z) : byte_scope.
Notation "#36" := (encode 54%Z) : byte_scope.
Notation "#37" := (encode 55%Z) : byte_scope.
Notation "#38" := (encode 56%Z) : byte_scope.
Notation "#39" := (encode 57%Z) : byte_scope.
Notation "#3A" := (encode 58%Z) : byte_scope.
Notation "#3B" := (encode 59%Z) : byte_scope.
Notation "#3C" := (encode 60%Z) : byte_scope.
Notation "#3D" := (encode 61%Z) : byte_scope.
Notation "#3E" := (encode 62%Z) : byte_scope.
Notation "#3F" := (encode 63%Z) : byte_scope.
Notation "#40" := (encode 64%Z) : byte_scope.
Notation "#41" := (encode 65%Z) : byte_scope.
Notation "#42" := (encode 66%Z) : byte_scope.
Notation "#43" := (encode 67%Z) : byte_scope.
Notation "#44" := (encode 68%Z) : byte_scope.
Notation "#45" := (encode 69%Z) : byte_scope.
Notation "#46" := (encode 70%Z) : byte_scope.
Notation "#47" := (encode 71%Z) : byte_scope.
Notation "#48" := (encode 72%Z) : byte_scope.
Notation "#49" := (encode 73%Z) : byte_scope.
Notation "#4A" := (encode 74%Z) : byte_scope.
Notation "#4B" := (encode 75%Z) : byte_scope.
Notation "#4C" := (encode 76%Z) : byte_scope.
Notation "#4D" := (encode 77%Z) : byte_scope.
Notation "#4E" := (encode 78%Z) : byte_scope.
Notation "#4F" := (encode 79%Z) : byte_scope.
Notation "#50" := (encode 80%Z) : byte_scope.
Notation "#51" := (encode 81%Z) : byte_scope.
Notation "#52" := (encode 82%Z) : byte_scope.
Notation "#53" := (encode 83%Z) : byte_scope.
Notation "#54" := (encode 84%Z) : byte_scope.
Notation "#55" := (encode 85%Z) : byte_scope.
Notation "#56" := (encode 86%Z) : byte_scope.
Notation "#57" := (encode 87%Z) : byte_scope.
Notation "#58" := (encode 88%Z) : byte_scope.
Notation "#59" := (encode 89%Z) : byte_scope.
Notation "#5A" := (encode 90%Z) : byte_scope.
Notation "#5B" := (encode 91%Z) : byte_scope.
Notation "#5C" := (encode 92%Z) : byte_scope.
Notation "#5D" := (encode 93%Z) : byte_scope.
Notation "#5E" := (encode 94%Z) : byte_scope.
Notation "#5F" := (encode 95%Z) : byte_scope.
Notation "#60" := (encode 96%Z) : byte_scope.
Notation "#61" := (encode 97%Z) : byte_scope.
Notation "#62" := (encode 98%Z) : byte_scope.
Notation "#63" := (encode 99%Z) : byte_scope.
Notation "#64" := (encode 100%Z) : byte_scope.
Notation "#65" := (encode 101%Z) : byte_scope.
Notation "#66" := (encode 102%Z) : byte_scope.
Notation "#67" := (encode 103%Z) : byte_scope.
Notation "#68" := (encode 104%Z) : byte_scope.
Notation "#69" := (encode 105%Z) : byte_scope.
Notation "#6A" := (encode 106%Z) : byte_scope.
Notation "#6B" := (encode 107%Z) : byte_scope.
Notation "#6C" := (encode 108%Z) : byte_scope.
Notation "#6D" := (encode 109%Z) : byte_scope.
Notation "#6E" := (encode 110%Z) : byte_scope.
Notation "#6F" := (encode 111%Z) : byte_scope.
Notation "#70" := (encode 112%Z) : byte_scope.
Notation "#71" := (encode 113%Z) : byte_scope.
Notation "#72" := (encode 114%Z) : byte_scope.
Notation "#73" := (encode 115%Z) : byte_scope.
Notation "#74" := (encode 116%Z) : byte_scope.
Notation "#75" := (encode 117%Z) : byte_scope.
Notation "#76" := (encode 118%Z) : byte_scope.
Notation "#77" := (encode 119%Z) : byte_scope.
Notation "#78" := (encode 120%Z) : byte_scope.
Notation "#79" := (encode 121%Z) : byte_scope.
Notation "#7A" := (encode 122%Z) : byte_scope.
Notation "#7B" := (encode 123%Z) : byte_scope.
Notation "#7C" := (encode 124%Z) : byte_scope.
Notation "#7D" := (encode 125%Z) : byte_scope.
Notation "#7E" := (encode 126%Z) : byte_scope.
Notation "#7F" := (encode 127%Z) : byte_scope.
Notation "#80" := (encode 128%Z) : byte_scope.
Notation "#81" := (encode 129%Z) : byte_scope.
Notation "#82" := (encode 130%Z) : byte_scope.
Notation "#83" := (encode 131%Z) : byte_scope.
Notation "#84" := (encode 132%Z) : byte_scope.
Notation "#85" := (encode 133%Z) : byte_scope.
Notation "#86" := (encode 134%Z) : byte_scope.
Notation "#87" := (encode 135%Z) : byte_scope.
Notation "#88" := (encode 136%Z) : byte_scope.
Notation "#89" := (encode 137%Z) : byte_scope.
Notation "#8A" := (encode 138%Z) : byte_scope.
Notation "#8B" := (encode 139%Z) : byte_scope.
Notation "#8C" := (encode 140%Z) : byte_scope.
Notation "#8D" := (encode 141%Z) : byte_scope.
Notation "#8E" := (encode 142%Z) : byte_scope.
Notation "#8F" := (encode 143%Z) : byte_scope.
Notation "#90" := (encode 144%Z) : byte_scope.
Notation "#91" := (encode 145%Z) : byte_scope.
Notation "#92" := (encode 146%Z) : byte_scope.
Notation "#93" := (encode 147%Z) : byte_scope.
Notation "#94" := (encode 148%Z) : byte_scope.
Notation "#95" := (encode 149%Z) : byte_scope.
Notation "#96" := (encode 150%Z) : byte_scope.
Notation "#97" := (encode 151%Z) : byte_scope.
Notation "#98" := (encode 152%Z) : byte_scope.
Notation "#99" := (encode 153%Z) : byte_scope.
Notation "#9A" := (encode 154%Z) : byte_scope.
Notation "#9B" := (encode 155%Z) : byte_scope.
Notation "#9C" := (encode 156%Z) : byte_scope.
Notation "#9D" := (encode 157%Z) : byte_scope.
Notation "#9E" := (encode 158%Z) : byte_scope.
Notation "#9F" := (encode 159%Z) : byte_scope.
Notation "#A0" := (encode 160%Z) : byte_scope.
Notation "#A1" := (encode 161%Z) : byte_scope.
Notation "#A2" := (encode 162%Z) : byte_scope.
Notation "#A3" := (encode 163%Z) : byte_scope.
Notation "#A4" := (encode 164%Z) : byte_scope.
Notation "#A5" := (encode 165%Z) : byte_scope.
Notation "#A6" := (encode 166%Z) : byte_scope.
Notation "#A7" := (encode 167%Z) : byte_scope.
Notation "#A8" := (encode 168%Z) : byte_scope.
Notation "#A9" := (encode 169%Z) : byte_scope.
Notation "#AA" := (encode 170%Z) : byte_scope.
Notation "#AB" := (encode 171%Z) : byte_scope.
Notation "#AC" := (encode 172%Z) : byte_scope.
Notation "#AD" := (encode 173%Z) : byte_scope.
Notation "#AE" := (encode 174%Z) : byte_scope.
Notation "#AF" := (encode 175%Z) : byte_scope.
Notation "#B0" := (encode 176%Z) : byte_scope.
Notation "#B1" := (encode 177%Z) : byte_scope.
Notation "#B2" := (encode 178%Z) : byte_scope.
Notation "#B3" := (encode 179%Z) : byte_scope.
Notation "#B4" := (encode 180%Z) : byte_scope.
Notation "#B5" := (encode 181%Z) : byte_scope.
Notation "#B6" := (encode 182%Z) : byte_scope.
Notation "#B7" := (encode 183%Z) : byte_scope.
Notation "#B8" := (encode 184%Z) : byte_scope.
Notation "#B9" := (encode 185%Z) : byte_scope.
Notation "#BA" := (encode 186%Z) : byte_scope.
Notation "#BB" := (encode 187%Z) : byte_scope.
Notation "#BC" := (encode 188%Z) : byte_scope.
Notation "#BD" := (encode 189%Z) : byte_scope.
Notation "#BE" := (encode 190%Z) : byte_scope.
Notation "#BF" := (encode 191%Z) : byte_scope.
Notation "#C0" := (encode 192%Z) : byte_scope.
Notation "#C1" := (encode 193%Z) : byte_scope.
Notation "#C2" := (encode 194%Z) : byte_scope.
Notation "#C3" := (encode 195%Z) : byte_scope.
Notation "#C4" := (encode 196%Z) : byte_scope.
Notation "#C5" := (encode 197%Z) : byte_scope.
Notation "#C6" := (encode 198%Z) : byte_scope.
Notation "#C7" := (encode 199%Z) : byte_scope.
Notation "#C8" := (encode 200%Z) : byte_scope.
Notation "#C9" := (encode 201%Z) : byte_scope.
Notation "#CA" := (encode 202%Z) : byte_scope.
Notation "#CB" := (encode 203%Z) : byte_scope.
Notation "#CC" := (encode 204%Z) : byte_scope.
Notation "#CD" := (encode 205%Z) : byte_scope.
Notation "#CE" := (encode 206%Z) : byte_scope.
Notation "#CF" := (encode 207%Z) : byte_scope.
Notation "#D0" := (encode 208%Z) : byte_scope.
Notation "#D1" := (encode 209%Z) : byte_scope.
Notation "#D2" := (encode 210%Z) : byte_scope.
Notation "#D3" := (encode 211%Z) : byte_scope.
Notation "#D4" := (encode 212%Z) : byte_scope.
Notation "#D5" := (encode 213%Z) : byte_scope.
Notation "#D6" := (encode 214%Z) : byte_scope.
Notation "#D7" := (encode 215%Z) : byte_scope.
Notation "#D8" := (encode 216%Z) : byte_scope.
Notation "#D9" := (encode 217%Z) : byte_scope.
Notation "#DA" := (encode 218%Z) : byte_scope.
Notation "#DB" := (encode 219%Z) : byte_scope.
Notation "#DC" := (encode 220%Z) : byte_scope.
Notation "#DD" := (encode 221%Z) : byte_scope.
Notation "#DE" := (encode 222%Z) : byte_scope.
Notation "#DF" := (encode 223%Z) : byte_scope.
Notation "#E0" := (encode 224%Z) : byte_scope.
Notation "#E1" := (encode 225%Z) : byte_scope.
Notation "#E2" := (encode 226%Z) : byte_scope.
Notation "#E3" := (encode 227%Z) : byte_scope.
Notation "#E4" := (encode 228%Z) : byte_scope.
Notation "#E5" := (encode 229%Z) : byte_scope.
Notation "#E6" := (encode 230%Z) : byte_scope.
Notation "#E7" := (encode 231%Z) : byte_scope.
Notation "#E8" := (encode 232%Z) : byte_scope.
Notation "#E9" := (encode 233%Z) : byte_scope.
Notation "#EA" := (encode 234%Z) : byte_scope.
Notation "#EB" := (encode 235%Z) : byte_scope.
Notation "#EC" := (encode 236%Z) : byte_scope.
Notation "#ED" := (encode 237%Z) : byte_scope.
Notation "#EE" := (encode 238%Z) : byte_scope.
Notation "#EF" := (encode 239%Z) : byte_scope.
Notation "#F0" := (encode 240%Z) : byte_scope.
Notation "#F1" := (encode 241%Z) : byte_scope.
Notation "#F2" := (encode 242%Z) : byte_scope.
Notation "#F3" := (encode 243%Z) : byte_scope.
Notation "#F4" := (encode 244%Z) : byte_scope.
Notation "#F5" := (encode 245%Z) : byte_scope.
Notation "#F6" := (encode 246%Z) : byte_scope.
Notation "#F7" := (encode 247%Z) : byte_scope.
Notation "#F8" := (encode 248%Z) : byte_scope.
Notation "#F9" := (encode 249%Z) : byte_scope.
Notation "#FA" := (encode 250%Z) : byte_scope.
Notation "#FB" := (encode 251%Z) : byte_scope.
Notation "#FC" := (encode 252%Z) : byte_scope.
Notation "#FD" := (encode 253%Z) : byte_scope.
Notation "#FE" := (encode 254%Z) : byte_scope.
Notation "#FF" := (encode 255%Z) : byte_scope.
